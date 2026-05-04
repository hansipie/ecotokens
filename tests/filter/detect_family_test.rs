use ecotokens::filter::detect_family;
use ecotokens::metrics::store::CommandFamily;

// --- Cas nominaux (commandes bare) ---

#[test]
fn bare_git() {
    assert_eq!(detect_family("git status"), CommandFamily::Git);
}

#[test]
fn bare_cargo() {
    assert_eq!(detect_family("cargo build"), CommandFamily::Cargo);
}

#[test]
fn bare_python() {
    assert_eq!(detect_family("python3 main.py"), CommandFamily::Python);
    assert_eq!(detect_family("pytest tests/"), CommandFamily::Python);
    assert_eq!(detect_family("uv run pytest"), CommandFamily::Python);
    assert_eq!(detect_family("ruff check ."), CommandFamily::Python);
}

#[test]
fn bare_js() {
    assert_eq!(detect_family("npm test"), CommandFamily::Js);
    assert_eq!(detect_family("npx jest"), CommandFamily::Js);
    assert_eq!(detect_family("pnpm build"), CommandFamily::Js);
}

// --- Chemins absolus ---

#[test]
fn absolute_path_git() {
    assert_eq!(detect_family("/usr/bin/git status"), CommandFamily::Git);
}

#[test]
fn absolute_path_cargo() {
    assert_eq!(
        detect_family("/usr/local/bin/cargo build"),
        CommandFamily::Cargo
    );
}

#[test]
fn absolute_path_python() {
    assert_eq!(
        detect_family("/usr/bin/python3 script.py"),
        CommandFamily::Python
    );
    assert_eq!(
        detect_family("/usr/bin/pytest tests/"),
        CommandFamily::Python
    );
}

// --- Chemins relatifs / venvs ---

#[test]
fn venv_pytest() {
    assert_eq!(
        detect_family(".venv/bin/pytest tests/"),
        CommandFamily::Python
    );
}

#[test]
fn venv_python() {
    assert_eq!(
        detect_family(".venv/bin/python -m pytest"),
        CommandFamily::Python
    );
}

#[test]
fn venv_pip() {
    assert_eq!(
        detect_family(".venv/bin/pip install -r requirements.txt"),
        CommandFamily::Python
    );
}

#[test]
fn venv_ruff() {
    assert_eq!(
        detect_family(".venv/bin/ruff check ."),
        CommandFamily::Python
    );
}

// --- Version managers ---

#[test]
fn pyenv_pip() {
    assert_eq!(
        detect_family("~/.pyenv/shims/pip install requests"),
        CommandFamily::Python
    );
}

#[test]
fn cargo_bin_path() {
    assert_eq!(
        detect_family("~/.cargo/bin/cargo build --release"),
        CommandFamily::Cargo
    );
}

// --- Wrappers poetry / pipx ---

#[test]
fn poetry_run() {
    assert_eq!(detect_family("poetry run pytest"), CommandFamily::Python);
}

#[test]
fn pipx_run() {
    assert_eq!(
        detect_family("pipx run ruff check ."),
        CommandFamily::Python
    );
}

// --- Node modules locaux ---

#[test]
fn local_jest() {
    assert_eq!(
        detect_family("./node_modules/.bin/jest --coverage"),
        CommandFamily::Js
    );
}

#[test]
fn local_eslint() {
    assert_eq!(
        detect_family("./node_modules/.bin/eslint src/"),
        CommandFamily::Js
    );
}

// --- Python version-specific ---

#[test]
fn python_versioned() {
    assert_eq!(detect_family("python3.11 -m pytest"), CommandFamily::Python);
    assert_eq!(
        detect_family("python3.12 -m pip install"),
        CommandFamily::Python
    );
}

// --- bash -c / sh -c wrapping ---

#[test]
fn bash_c_git() {
    assert_eq!(detect_family("bash -c \"git status\""), CommandFamily::Git);
}

#[test]
fn bash_c_cargo() {
    assert_eq!(
        detect_family("bash -c 'cargo build --release'"),
        CommandFamily::Cargo
    );
}

#[test]
fn sh_c_npm() {
    assert_eq!(detect_family("sh -c \"npm test\""), CommandFamily::Js);
}

#[test]
fn zsh_c_python() {
    assert_eq!(
        detect_family("zsh -c \"python3 script.py\""),
        CommandFamily::Python
    );
}

#[test]
fn bash_e_c_git() {
    // Flags supplémentaires avant -c
    assert_eq!(
        detect_family("bash -e -c \"git log --oneline\""),
        CommandFamily::Git
    );
}

#[test]
fn bash_c_unquoted() {
    assert_eq!(detect_family("bash -c cargo test"), CommandFamily::Cargo);
}

#[test]
fn bash_c_unknown_stays_generic() {
    assert_eq!(
        detect_family("bash -c \"bundle exec rspec\""),
        CommandFamily::Generic
    );
}

// --- Cas qui doivent rester Generic ---

#[test]
fn unknown_stays_generic() {
    assert_eq!(detect_family("bundle exec rspec"), CommandFamily::Generic);
    assert_eq!(detect_family("./myscript.sh"), CommandFamily::Generic);
}
