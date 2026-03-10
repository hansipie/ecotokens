use ecotokens::filter::python::filter_python;

#[test]
fn pytest_with_failures_keeps_failures() {
    let input = "============================= test session starts ==============================
platform linux -- Python 3.10.12, pytest-7.4.0, pluggy-1.2.0
rootdir: /home/hansi/Code/ecotokens
collected 5 items

tests/test_foo.py .F...                                                   [100%]

=================================== FAILURES ===================================
__________________________________ test_fail ___________________________________

    def test_fail():
>       assert 1 == 2
E       assert 1 == 2
E         +1
E         -2

tests/test_foo.py:6: AssertionError
=========================== short test summary info ============================
FAILED tests/test_foo.py::test_fail - assert 1 == 2
========================= 1 failed, 4 passed in 0.05s ==========================
";
    let out = filter_python("pytest", input);
    assert!(out.contains("FAILURES"), "FAILURES header should be kept");
    assert!(out.contains("test_fail"), "failed test name should be kept");
    assert!(
        out.contains("E       assert 1 == 2"),
        "assertion error should be kept"
    );
    assert!(
        out.contains("1 failed, 4 passed"),
        "summary line should be kept"
    );
}

#[test]
fn pip_install_summarizes_long_output() {
    let mut input = String::new();
    for i in 0..50 {
        input.push_str(&format!(
            "Collecting pkg{i}\n  Downloading pkg{i}-1.0.tar.gz (10kB)\n"
        ));
    }
    input.push_str("Successfully installed pkg1 pkg2 pkg3 ...\n");
    let out = filter_python("pip install .", &input);
    assert!(
        out.contains("Successfully installed"),
        "success line should be kept"
    );
    assert!(
        !out.contains("Collecting pkg10"),
        "intermediate noise should be omitted"
    );
}

#[test]
fn ruff_concise_output_is_passed_through() {
    let input =
        "src/main.py:10:5: F841 local variable 'x' is assigned to but never used\nFound 1 error.\n";
    let out = filter_python("ruff check .", input);
    assert_eq!(out, input, "short ruff output should be kept as is");
}

#[test]
fn pytest_failures_include_traceback() {
    // Verify that the traceback inside the FAILURES section is preserved
    let input = "============================= test session starts ==============================
collected 1 item

=================================== FAILURES ===================================
__________________________________ test_fail ___________________________________

    def test_fail():
>       assert 1 == 2
E       assert 1 == 2

tests/test_foo.py:6: AssertionError
=========================== short test summary info ============================
FAILED tests/test_foo.py::test_fail - assert 1 == 2
========================= 1 failed in 0.05s ========================
";
    let out = filter_python("pytest", input);
    assert!(
        out.contains("FAILURES"),
        "FAILURES section header should be kept"
    );
    assert!(
        out.contains("def test_fail"),
        "traceback body should be kept"
    );
    assert!(
        out.contains("AssertionError"),
        "error location should be kept"
    );
}

#[test]
fn python_m_pytest_is_routed_correctly() {
    let input = "============================= test session starts ==============================
collected 1 item

tests/test_foo.py .                                                      [100%]

========================= 1 passed in 0.01s ========================
";
    // Should not panic and should return something reasonable
    let out = filter_python("python -m pytest tests/", input);
    assert!(out.contains("passed"), "summary line should be present");
}

#[test]
fn uv_run_pytest_is_routed_correctly() {
    let input = "============================= test session starts ==============================
collected 1 item

=================================== FAILURES ===================================
__________________________________ test_x __________________________________

    def test_x():
>       assert False
E       assert False

tests/test_x.py:2: AssertionError
=========================== short test summary info ============================
FAILED tests/test_x.py::test_x - assert False
========================= 1 failed in 0.03s ========================
";
    let out = filter_python("uv run pytest", input);
    assert!(
        out.contains("FAILURES"),
        "uv run pytest should route through pytest filter"
    );
    assert!(out.contains("assert False"), "assertion should be kept");
}

#[test]
fn uv_pip_install_is_routed_correctly() {
    let mut input = String::new();
    for i in 0..30 {
        input.push_str(&format!(
            "Collecting dep{i}\n  Downloading dep{i}-1.0.whl\n"
        ));
    }
    input.push_str("Successfully installed dep1 dep2\n");
    let out = filter_python("uv pip install .", &input);
    assert!(
        out.contains("Successfully installed"),
        "uv pip should route through pip filter"
    );
    assert!(
        !out.contains("Collecting dep10"),
        "download noise should be omitted"
    );
}

#[test]
fn pytest_collection_error_is_kept() {
    let input = "============================= test session starts ==============================
collected 0 items / 1 error

=================================== ERRORS ====================================
__________________ ERROR collecting tests/test_broken.py ___________________
ImportError while importing test module '/home/user/tests/test_broken.py'.
E   ModuleNotFoundError: No module named 'mylib'
=========================== short test summary info ============================
ERROR tests/test_broken.py - ModuleNotFoundError: No module named 'mylib'
======================== 1 error in 0.12s =========================
";
    let out = filter_python("pytest", input);
    assert!(out.contains("ERRORS"), "ERRORS section should be kept");
    assert!(
        out.contains("ModuleNotFoundError"),
        "import error detail should be kept"
    );
}

#[test]
fn pip_error_is_kept() {
    let mut input = String::new();
    for i in 0..25 {
        input.push_str(&format!(
            "Collecting dep{i}\n  Downloading dep{i}-1.0.whl (10kB)\n"
        ));
    }
    input.push_str("ERROR: Could not find a version that satisfies the requirement nosuchpkg (from versions: none)\n");
    input.push_str("ERROR: No matching distribution found for nosuchpkg\n");
    let out = filter_python("pip install nosuchpkg", &input);
    assert!(out.contains("ERROR:"), "pip error lines should be kept");
    assert!(
        !out.contains("Downloading dep10"),
        "download noise should be omitted"
    );
}

#[test]
fn ruff_many_errors_keeps_all_structured_lines() {
    let mut input = String::new();
    for i in 0..120 {
        input.push_str(&format!(
            "src/file{i}.py:{i}:1: F401 `os` imported but unused\n"
        ));
    }
    input.push_str("Found 120 errors.\n");
    let out = filter_python("ruff check .", &input);
    // All diagnostic lines should be kept (no head+tail truncation in the middle)
    assert!(
        out.contains("src/file60.py"),
        "middle errors should not be truncated by head+tail"
    );
    assert!(
        out.contains("Found 120 errors."),
        "summary line should be kept"
    );
}
