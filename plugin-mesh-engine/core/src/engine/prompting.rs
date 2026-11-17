//! Промпт-шаблоны концепт-изображений для text→3D пайплайна (SSOT, решение
//! владельца 17.11.26: «шаблоны должны быть в плагине лежать»). Шаблоны —
//! дословно из документа специалиста (sd.cpp/Z-Image-Turbo: при cfg-scale 1.0
//! negative-prompt НЕ работает → все запреты в теле промпта; квадрат 1024,
//! мягкий свет, поля от краёв, 3/4 ракурс; деревья — «chunky foliage» вместо
//! «thousands of tiny leaves»). Хост не переопределяет тексты: тул
//! `mesh_concept_prompt` подставляет английское описание объекта в шаблон
//! выбранной категории, результат идёт в `generate_image` как `prompt_en`.

/// Категория концепт-шаблона (тип объекта в запросе пользователя).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConceptKind {
    /// Предметы: камни, сундуки, бочки, дома, заборы, руины.
    Prop,
    /// Деревья, кусты, растения.
    Tree,
    /// Оружие, щиты, инструменты, инвентарь.
    Weapon,
    /// Персонажи, монстры.
    Character,
}

impl ConceptKind {
    /// Разбор ключа категории из аргумента тула (честная ошибка-инструкция).
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "prop" => Ok(ConceptKind::Prop),
            "tree" => Ok(ConceptKind::Tree),
            "weapon" => Ok(ConceptKind::Weapon),
            "character" => Ok(ConceptKind::Character),
            _ => Err(format!(
                "Неизвестная категория '{}'. Доступны: prop, tree, weapon, character.",
                raw
            )),
        }
    }

    /// Ключ категории (как в аргументе тула).
    pub fn key(&self) -> &'static str {
        match self {
            ConceptKind::Prop => "prop",
            ConceptKind::Tree => "tree",
            ConceptKind::Weapon => "weapon",
            ConceptKind::Character => "character",
        }
    }
}

/// Каталог категорий для описания тула: (ключ, человекочитаемое описание).
/// Живёт рядом с шаблонами — агент выбирает категорию по нему.
pub const CONCEPT_KIND_CATALOG: &[(&str, &str)] = &[
    (
        "prop",
        "предметы: камни, сундуки, бочки, дома, заборы, руины, мебель",
    ),
    ("tree", "деревья, кусты, растения"),
    ("weapon", "оружие, щиты, инструменты, предметы инвентаря"),
    ("character", "персонажи, монстры, существа"),
];

const CONCEPT_TEMPLATE_PROP: &str = "Single isolated 3D game asset of {USER_PROMPT}, centered in frame with generous white margin around all edges, three-quarter isometric perspective view showing depth and sides, clean solid silhouette, detailed PBR surface texture, soft even diffuse studio lighting, no harsh shadows, no cast shadow on floor, no ground plane, isolated on pure solid flat #FFFFFF white background, sharp focus everywhere, no bokeh, no extra objects, clean 3D render";

const CONCEPT_TEMPLATE_TREE: &str = "Single isolated 3D game asset of {USER_PROMPT}, thick sturdy trunk and dense chunky sculpted foliage clusters forming a clean readable silhouette, solid leaf clumps, no tiny floating twigs, no translucent wispy leaves, centered in frame with wide empty margin on all four sides, three-quarter slightly elevated view, soft even shadowless studio lighting, no cast shadow, no ground grass patch, isolated on pure solid flat #FFFFFF white background, sharp focus, 3D game model render";

const CONCEPT_TEMPLATE_WEAPON: &str = "Single isolated 3D game prop of {USER_PROMPT}, angled in three-quarter 45-degree perspective to clearly show thickness and depth, full item completely inside frame with wide white margin, crisp clean geometry, rich PBR material detail, soft even studio lighting, no bloom, no lens flare, no harsh reflections, no floor shadow, isolated on pure solid flat #FFFFFF white background, sharp focus everywhere, 3D asset render";

const CONCEPT_TEMPLATE_CHARACTER: &str = "Full body single 3D character game asset of {USER_PROMPT}, standing upright facing directly forward, symmetrical A-pose with arms angled away from torso and fingers separated, legs straight and slightly apart, head to boots completely inside frame with wide white margin, soft even flat studio lighting, no harsh shadows, no pedestal, no floor shadow, isolated on pure solid flat #FFFFFF white background, sharp crisp details everywhere, clean 3D render";

/// Собрать промпт концепт-изображения: подставить английское описание объекта
/// в шаблон категории. Описание обязано быть непустым (после trim) — честная
/// ошибка, а не пустая дыра в промпте.
pub fn build_concept_prompt(subject_en: &str, kind: ConceptKind) -> Result<String, String> {
    let subject = subject_en.trim();
    if subject.is_empty() {
        return Err(
            "subject_en пуст: опиши объект по-английски (сам объект, без фона и сцены)"
                .to_string(),
        );
    }
    let template = match kind {
        ConceptKind::Prop => CONCEPT_TEMPLATE_PROP,
        ConceptKind::Tree => CONCEPT_TEMPLATE_TREE,
        ConceptKind::Weapon => CONCEPT_TEMPLATE_WEAPON,
        ConceptKind::Character => CONCEPT_TEMPLATE_CHARACTER,
    };
    Ok(template.replace("{USER_PROMPT}", subject))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_contain_specialist_markers() {
        // Дословные маркеры документа специалиста: запреты в теле промпта
        // (cfg 1.0 → негатив не работает), белый фон, поля, ракурсы.
        let cases: Vec<(&str, Vec<&str>)> = vec![
            (
                CONCEPT_TEMPLATE_PROP,
                vec![
                    "#FFFFFF",
                    "three-quarter isometric perspective",
                    "no cast shadow on floor",
                    "no bokeh",
                ],
            ),
            (
                CONCEPT_TEMPLATE_TREE,
                vec![
                    "dense chunky sculpted foliage clusters",
                    "solid leaf clumps",
                    "no tiny floating twigs",
                    "#FFFFFF",
                ],
            ),
            (
                CONCEPT_TEMPLATE_WEAPON,
                vec!["45-degree perspective", "no bloom, no lens flare", "#FFFFFF"],
            ),
            (
                CONCEPT_TEMPLATE_CHARACTER,
                vec![
                    "symmetrical A-pose",
                    "fingers separated",
                    "standing upright facing directly forward",
                    "#FFFFFF",
                ],
            ),
        ];
        for (template, markers) in &cases {
            for marker in markers {
                assert!(
                    template.contains(*marker),
                    "шаблон '{}' потерял маркер '{}'",
                    kind_of(template),
                    marker
                );
            }
        }
    }

    fn kind_of(template: &str) -> &'static str {
        if template.contains("A-pose") {
            "character"
        } else if template.contains("foliage") {
            "tree"
        } else if template.contains("45-degree") {
            "weapon"
        } else {
            "prop"
        }
    }

    #[test]
    fn build_substitutes_subject_and_leaves_no_placeholder() {
        let prompt = build_concept_prompt("an ancient stone chest", ConceptKind::Prop)
            .expect("валидная сборка");
        assert!(prompt.contains("an ancient stone chest"));
        assert!(!prompt.contains("{USER_PROMPT}"));
        assert!(prompt.starts_with("Single isolated 3D game asset of an ancient stone chest"));
    }

    #[test]
    fn build_gives_distinct_prompts_per_kind() {
        let subject = "a mushroom";
        let prop = build_concept_prompt(subject, ConceptKind::Prop).unwrap();
        let tree = build_concept_prompt(subject, ConceptKind::Tree).unwrap();
        let weapon = build_concept_prompt(subject, ConceptKind::Weapon).unwrap();
        let character = build_concept_prompt(subject, ConceptKind::Character).unwrap();
        assert_ne!(prop, tree);
        assert_ne!(prop, weapon);
        assert_ne!(prop, character);
        assert_ne!(tree, weapon);
        assert_ne!(tree, character);
        assert_ne!(weapon, character);
    }

    #[test]
    fn empty_subject_is_honest_error() {
        assert!(build_concept_prompt("", ConceptKind::Prop).is_err());
        assert!(build_concept_prompt("   \n\t ", ConceptKind::Tree).is_err());
    }

    #[test]
    fn parse_accepts_catalog_keys_and_rejects_junk() {
        for (key, _) in CONCEPT_KIND_CATALOG {
            assert!(
                ConceptKind::parse(key).is_ok(),
                "ключ каталога '{}' не парсится",
                key
            );
        }
        assert!(ConceptKind::parse("spaceship").is_err());
        assert!(ConceptKind::parse("").is_err());
        assert!(ConceptKind::parse("PROP").is_err()); // регистр значим: тула отдаёт enum
    }

    #[test]
    fn catalog_keys_match_parse() {
        let expected = ["prop", "tree", "weapon", "character"];
        assert_eq!(CONCEPT_KIND_CATALOG.len(), expected.len());
        for (i, (key, _)) in CONCEPT_KIND_CATALOG.iter().enumerate() {
            assert_eq!(*key, expected[i]);
            assert_eq!(ConceptKind::parse(key).unwrap().key(), expected[i]);
        }
    }
}
