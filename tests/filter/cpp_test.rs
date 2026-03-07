use ecotokens::filter::cpp::filter_cpp;
use std::process::Command;
use tempfile::TempDir;

fn ecotokens() -> String {
    env!("CARGO_BIN_EXE_ecotokens").to_string()
}

#[test]
fn compiler_errors_are_preserved() {
    let input = "src/main.c:12:5: error: use of undeclared identifier 'value'\n   12 |     value = 42;\n      |     ^\n1 error generated.\n";
    let out = filter_cpp("clang main.c", input);
    assert!(out.contains("error:"), "compiler errors should be kept");
    assert!(out.contains("undeclared identifier"), "diagnostic details should be kept");
    assert!(out.contains("1 error generated."), "summary line should be kept");
}

#[test]
fn many_warnings_are_summarized() {
    let mut input = String::new();
    for i in 0..25 {
        input.push_str(&format!(
            "src/file{i}.c:{i}:3: warning: unused variable 'tmp{i}' [-Wunused-variable]\n"
        ));
    }
    input.push_str("25 warnings generated.\n");
    input.push_str("build completed successfully\n");

    let out = filter_cpp("gcc -Wall main.c", &input);
    assert!(out.len() < input.len(), "warning-heavy output should be reduced");
    assert!(out.contains("25 warnings generated."), "summary line should be kept");
}

#[test]
fn short_output_passes_through() {
    let input = "clang++ -std=c++20 main.cpp\nBuild succeeded.\n";
    let out = filter_cpp("clang++ main.cpp", input);
    assert_eq!(out, input, "short non-diagnostic output should pass through");
}

#[test]
fn filter_command_routes_gpp_through_cpp_filter() {
    let temp = TempDir::new().unwrap();
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let script_path = bin_dir.join("g++");
    std::fs::write(
        &script_path,
        "#!/bin/sh\ni=1\nwhile [ \"$i\" -le 30 ]; do\n  echo \"src/main.cpp:$i:5: warning: unused variable 'tmp$i' [-Wunused-variable]\"\n  i=$((i + 1))\ndone\necho \"30 warnings generated.\"\n",
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();
    }

    let current_path = std::env::var("PATH").unwrap_or_default();
    let combined_path = format!("{}:{}", bin_dir.display(), current_path);

    let out = Command::new(ecotokens())
        .args(["filter", "--", "g++", "main.cpp"])
        .env("PATH", combined_path)
        .output()
        .expect("failed to run ecotokens filter");

    assert!(out.status.success(), "filter should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.len() < 1200,
        "C++ filter should summarize warning-heavy compiler output"
    );
    assert!(
        stdout.contains("30 warnings generated."),
        "compiler summary should be retained"
    );
}

#[test]
fn make_native_errors_are_detected() {
    let input = "make[1]: Entering directory '/home/user/project'\ngcc -c src/foo.c\nsrc/foo.c:5:1: error: unknown type name 'Foo'\nmake[2]: *** [Makefile:12: src/foo.o] Error 1\nmake[1]: *** [Makefile:8: all] Error 2\n";
    let out = filter_cpp("make", input);
    assert!(out.contains("error:"), "compiler errors should be kept");
    assert!(out.contains("make: ***") || out.contains("make[2]: ***"), "make error line should be kept");
}

#[test]
fn cmake_errors_are_detected() {
    let input = "-- Configuring done\n-- Build files have been written to: /tmp/build\nCMake Error at CMakeLists.txt:10 (target_link_libraries):\n  Cannot specify link libraries for target \"app\" which is not built by this\n  project.\n";
    let out = filter_cpp("cmake ..", input);
    assert!(out.contains("CMake Error"), "CMake error line should be kept");
}

#[test]
fn linker_errors_are_detected() {
    let input = "gcc -o app main.o util.o\nld: error: undefined symbol: foo_init\n>>> referenced by main.c:5\n1 error generated.\n";
    let out = filter_cpp("gcc -o app main.o util.o", input);
    assert!(out.contains("ld: error:") || out.contains("undefined symbol"), "linker error should be kept");
}

#[test]
fn mixed_errors_and_warnings_keeps_both() {
    let mut input = String::new();
    for i in 0..15 {
        input.push_str(&format!(
            "src/lib.c:{i}:3: warning: implicit declaration of function 'bar{i}' [-Wimplicit-function-declaration]\n"
        ));
    }
    input.push_str("src/lib.c:20:1: error: use of undeclared identifier 'baz'\n");
    input.push_str("15 warnings and 1 error generated.\n");

    let out = filter_cpp("gcc -Wall src/lib.c", &input);
    assert!(out.contains("error:"), "error line should be kept");
    assert!(out.contains("summarized") || out.len() < input.len(), "output should be shorter for many warnings");
}
