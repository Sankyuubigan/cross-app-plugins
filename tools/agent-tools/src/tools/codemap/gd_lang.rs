//! GDScript: легковесный структурный парсер по образцу [`super::md_lang`].
//!
//! ## Что здесь разбирается
//!
//! GDScript — язык с отступами, но карте не нужен полный AST: достаточно
//! объявлений верхнего уровня и упоминаний, которые создают рёбра графа.
//!
//! - **Определения**: `func`, `class_name`, `signal`, `const`, `enum`.
//!   Вложенные функции и методы классов тоже попадают в карту — как методы
//!   в [`super::py_lang`]. `var` определением не считается: поля класса —
//!   не символ уровня репозитория, их сотни, и в карте они были бы шумом.
//! - **Ссылки**: обращение через точку (`.play_jump()`), типы после `:`,
//!   проверки `is`/`as`, наследование `extends`, и пути из
//!   `preload()`/`load()`/`extends "res://…"`.
//!
//! ## Почему файл сам себе определение
//!
//! В GDScript класс может быть безымянным: `class_name` необязателен, и в
//! реальных проектах (в отличие от библиотек) его чаще нет. Де-факто модуль
//! идентифицируется именем файла: `preload("res://…/WFCSolver.gd")` и
//! присваивается константе с тем же именем. Поэтому каждый файл даёт
//! определение с именем своего стема, а `preload`/`load` со ссылкой на
//! `.gd`-путь дают ссылку на стем цели. Без этого проекты в стиле
//! «константа = preload» не имели бы в графе ни одного межфайлового ребра.
//!
//! ## Чего здесь НЕТ, и это осознанно
//!
//! - **Полного AST**: разбор построчный, без дерева. Код внутри многострочных
//!   строк может дать ложную ссылку — граф её отсечёт отсутствием определения.
//! - **Различения чтения и записи** (как в `py_lang` через `ExprContext`):
//!   без AST это невозможно; цена — лишние ссылки, но не ложные рёбра,
//!   потому что ребро всё равно требует пересечения с определением.
//! - **Ссылок по node-path** (`get_node("…/AnimationPlayer")`): это пути
//!   дерева сцены, а не символы кода — смешивать их с графом вызовов
//!   значило бы врать о структуре.

use super::tags::Tag;

/// Расширения, которые не считаются обращением к символу.
///
/// Без них путь `"res://Scenes/player.tscn"` внутри `load()` давал бы ссылку
/// на «символ» `tscn`: точка перед расширением неотличима от точки перед
/// методом. Список — то, что реально встречается в строках GDScript-проектов.
const EXTENSION_LOOKALIKES: [&str; 12] = [
    "gd", "tscn", "tres", "res", "webp", "fbx", "png", "wav", "ogg", "import", "godot", "json",
];

/// Извлечь определения и ссылки из исходника GDScript.
pub fn tags(rel_path: &str, source: &str) -> Vec<Tag> {
    let mut out = Vec::new();

    // Модуль файла: безымянный класс де-факто зовётся как файл (см. шапку).
    let stem = file_stem(rel_path);
    out.push(Tag::def(rel_path, &stem, 1));

    for (index, line) in source.lines().enumerate() {
        let line_no = (index + 1) as u32;
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if let Some(name) = def_name(trimmed) {
            out.push(Tag::def(rel_path, &name, line_no));
        }
        refs_of(trimmed, rel_path, line_no, &mut out);
    }
    out
}

/// Имя определения в строке, либо `None`.
///
/// Расширение `_name` в конце допускается: GDScript-конвенция для приватных
/// методов (`_ready`, `_process`) — такие функции обязаны попасть в карту.
fn def_name(line: &str) -> Option<String> {
    let rest = ["static func ", "func "]
        .iter()
        .find_map(|kw| line.strip_prefix(kw))
        .or_else(|| line.strip_prefix("class_name "))
        .or_else(|| line.strip_prefix("signal "))
        .or_else(|| line.strip_prefix("const "))
        .or_else(|| line.strip_prefix("enum "))?;
    let name = rest.split(['(', ':', '=', '{']).next()?.trim();
    ident_ok(name).then(|| name.to_string())
}

/// Ссылки строки: пути, атрибуты, типы, `is`/`as`/`extends`.
fn refs_of(line: &str, rel_path: &str, line_no: u32, out: &mut Vec<Tag>) {
    resource_refs(line, rel_path, line_no, out);
    attribute_refs(line, rel_path, line_no, out);
    for keyword in ["extends", "is", "as"] {
        typed_refs_after(line, keyword, rel_path, line_no, out);
    }
    typed_refs_after(line, ":", rel_path, line_no, out);
}

/// `preload("res://…/Foo.gd")`, `load(…)`, `extends "res://…/Foo.gd"`.
///
/// Ссылка называется **стемом цели**: в GDScript безымянный класс де-факто
/// зовётся именем файла (см. шапку модуля), и именно так место вызова
/// связывается с файлом, у которого нет `class_name`.
fn resource_refs(line: &str, rel_path: &str, line_no: u32, out: &mut Vec<Tag>) {
    for keyword in ["preload(", "load(", "extends \""] {
        let mut from = 0;
        while let Some(found) = line[from..].find(keyword) {
            let tail = &line[from + found + keyword.len()..];
            if keyword == "extends \"" || keyword.starts_with("preload") || keyword.starts_with("load") {
                if let Some(path) = quoted_prefix(tail) {
                    if path.ends_with(".gd") {
                        out.push(Tag::reference(rel_path, &file_stem(&path), line_no));
                    }
                }
            }
            from += found + keyword.len();
        }
    }
}

/// Строка до закрывающей кавычки, либо `None`.
fn quoted_prefix(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('"')?;
    rest.split('"').next()
}

/// Обращения через точку: `.play_jump()`, `.velocity`.
///
/// `1.5` и `..` отсекаются требованием буквенного начала и конца идентификатора.
fn attribute_refs(line: &str, rel_path: &str, line_no: u32, out: &mut Vec<Tag>) {
    let bytes = line.as_bytes();
    for (i, ch) in bytes.iter().enumerate() {
        if *ch != b'.' {
            continue;
        }
        let start = i + 1;
        if start >= bytes.len() || !(bytes[start].is_ascii_alphabetic() || bytes[start] == b'_') {
            continue;
        }
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        // До точки не должно быть части числа или другого идентификатора.
        if i > 0 && (bytes[i - 1].is_ascii_digit() || bytes[i - 1] == b'.') {
            continue;
        }
        let name = &line[start..end];
        if EXTENSION_LOOKALIKES.contains(&name) {
            continue;
        }
        out.push(Tag::reference(rel_path, name, line_no));
    }
}

/// Идентификатор с заглавной буквы после ключевого слова или `:`.
///
/// Регистр — фильтр, а не гарантия: в GDScript классы и константы-типы
/// пишутся с заглавной (`HealthComponent`, `SkillDB`), ключевые слова — нет.
fn typed_refs_after(line: &str, keyword: &str, rel_path: &str, line_no: u32, out: &mut Vec<Tag>) {
    let mut from = 0;
    while let Some(found) = line[from..].find(keyword) {
        let tail = &line[from + found + keyword.len()..];
        let tail = tail.trim_start();
        let name: String = tail
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            out.push(Tag::reference(rel_path, &name, line_no));
        }
        from += found + keyword.len();
    }
}

/// Идентификатор пригоден как имя: буква/подчёркивание в начале.
fn ident_ok(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

/// Стем пути файла: `Logic/Skills/Skill.gd` → `Skill`.
fn file_stem(rel_path: &str) -> &str {
    let name = rel_path.rsplit('/').next().unwrap_or(rel_path);
    name.split('.').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::codemap::tags::TagKind;

    /// Нумерация строк значима для тестов, пустые строки не удалять.
    const SOURCE: &str = "\
extends Node
const SkillDB = preload(\"res://Logic/Skills/SkillDB.gd\")

signal hit(amount)

enum State { IDLE, RUN }

func _ready():
    var caster = SpellCasterComponent.new()

func play_jump():
    anim_comp.play_jump()
";

    fn parse() -> Vec<Tag> {
        tags("Logic/Player.gd", SOURCE)
    }

    fn def_line(tags: &[Tag], name: &str) -> u32 {
        tags.iter()
            .find(|t| t.kind == TagKind::Def && t.name == name)
            .unwrap_or_else(|| panic!("нет определения {name}"))
            .line
    }

    fn ref_lines(tags: &[Tag], name: &str) -> Vec<u32> {
        tags.iter()
            .filter(|t| t.kind == TagKind::Ref && t.name == name)
            .map(|t| t.line)
            .collect()
    }

    /// ГЕЙТ: файл без class_name всё равно имеет имя модуля — иначе проекты
    /// в стиле «константа = preload» оставались бы без межфайловых рёбер.
    #[test]
    fn file_stem_is_a_module_definition() {
        let tags = parse();
        assert_eq!(def_line(&tags, "Player"), 1);
    }

    #[test]
    fn collects_all_declaration_kinds() {
        let tags = parse();
        assert_eq!(def_line(&tags, "hit"), 4, "signal — определение");
        assert_eq!(def_line(&tags, "State"), 6, "enum — определение");
        assert_eq!(def_line(&tags, "_ready"), 8, "метод с подчёркиванием — в карте");
        assert_eq!(def_line(&tags, "play_jump"), 11);
    }

    #[test]
    fn const_is_a_definition_even_with_preload() {
        let tags = parse();
        assert_eq!(def_line(&tags, "SkillDB"), 2);
    }

    #[test]
    fn preload_links_to_target_file_stem() {
        let tags = parse();
        // Прелоад-путь обязан дать ссылку на стем цели: без этого проекты в
        // стиле «константа = preload» не имели бы ни одного межфайлового ребра.
        assert!(ref_lines(&tags, "SkillDB").contains(&2), "preload — ссылка на стем цели");
    }

    #[test]
    fn attribute_calls_become_references() {
        let tags = parse();
        assert_eq!(ref_lines(&tags, "play_jump"), vec![12], "вызов .play_jump() — строка 12");
    }

    #[test]
    fn extends_becomes_a_reference() {
        let tags = parse();
        assert!(ref_lines(&tags, "Node").contains(&1), "extends Node — ссылка");
    }

    #[test]
    fn float_literals_are_not_attributes() {
        let tags = tags("probe.gd", "var speed = 1.5\nvar step = 2.25\n");
        assert!(
            tags.iter().all(|t| t.name != "5" && t.name != "25"),
            "дробные литералы не должны распознаваться как ссылки: {tags:?}"
        );
    }

    #[test]
    fn comments_and_extension_lookalikes_are_skipped() {
        let tags = tags(
            "probe.gd",
            "# func fake():\nvar scene = load(\"res://Scenes/player.tscn\")\n",
        );
        assert!(tags.iter().all(|t| t.name != "fake"), "комментарий — не определение");
        assert!(tags.iter().all(|t| t.name != "tscn"), "расширение — не символ");
    }

    #[test]
    fn type_annotations_become_references() {
        let tags = tags("probe.gd", "var hp: HealthComponent\n");
        assert!(ref_lines(&tags, "HealthComponent").contains(&1));
    }

    #[test]
    fn empty_source_yields_only_module_definition() {
        let tags = tags("empty.gd", "");
        assert_eq!(tags.len(), 1, "определение модуля остаётся: {tags:?}");
        assert_eq!(tags[0].name, "empty");
    }
}
