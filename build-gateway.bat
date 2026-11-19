@echo off
setlocal enableextensions

rem Сборка release-бинаря шлюза cloud-routers-gateway.
rem Окружение — по образцу test-plugin.bat: vswhere → vcvarsall x64
rem (прямой cargo-запуск без MSVC-инициализации запрещён правилами сборки).
rem
rem Место результата: D:\Programs\cargo-target\release\cloud-routers-gateway.exe —
rem глобальный cargo target-dir (16.11.2026, ~/.cargo/config.toml → build.target-dir)
rem уводит все сборки в общий каталог; workspace target/ остаётся пустым.
rem Панель King Orch находит бинарь через source_candidates, который с
rem 19.11.2026 проверяет и глобальный target-dir: после пересборки «Проверить
rem обновления» сравнит версию/mtime/размер и предложит «Обновить сейчас».

for /f "usebackq delims=" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -property installationPath 2^>nul`) do (
    if exist "%%i\VC\Auxiliary\Build\vcvarsall.bat" (
        call "%%i\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1
        echo VS=%%i
    )
)

cargo build -p cloud-routers-gateway --release
set EXIT=%ERRORLEVEL%
echo EXIT=%EXIT%
exit /b %EXIT%
