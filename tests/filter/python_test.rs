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
    assert!(out.contains("E       assert 1 == 2"), "assertion error should be kept");
    assert!(out.contains("1 failed, 4 passed"), "summary line should be kept");
}

#[test]
fn pip_install_summarizes_long_output() {
    let mut input = String::new();
    for i in 0..50 {
        input.push_str(&format!("Collecting pkg{i}\n  Downloading pkg{i}-1.0.tar.gz (10kB)\n"));
    }
    input.push_str("Successfully installed pkg1 pkg2 pkg3 ...\n");
    let out = filter_python("pip install .", &input);
    assert!(out.contains("Successfully installed"), "success line should be kept");
    assert!(!out.contains("Collecting pkg10"), "intermediate noise should be omitted");
}

#[test]
fn ruff_concise_output_is_passed_through() {
    let input = "src/main.py:10:5: F841 local variable 'x' is assigned to but never used\nFound 1 error.\n";
    let out = filter_python("ruff check .", input);
    assert_eq!(out, input, "short ruff output should be kept as is");
}
