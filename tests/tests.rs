mod inputs;
mod macros;

use std::fs;

use assert_cmd::Command;
use tempdir::TempDir;

use inputs::*;
use macros::*;

decor_test!(
    prerender_generates_html,
    NO_JS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    dom_render_does_not_generate_html,
    NO_JS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("-r").arg("csr");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    fails_with_invalid_render_method,
    NO_JS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("-r").arg("invalid");
        cmd.assert().failure();
        assert_all!(dir.path());
    }
);

decor_test!(
    can_generate_index_html_that_integrates_with_prerender,
    NO_JS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--html");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    css_is_integrated_in_index_html,
    CSS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--html");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    can_change_out_file_stem,
    NO_JS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("-o").arg("new");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    can_change_out_file_stem_that_updates_in_index_html,
    NO_JS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("-o").arg("new").arg("--html");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    css_file_in_generated_prerender_method,
    CSS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    css_file_is_generated_dom_method,
    CSS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("-r").arg("csr");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    wasm_for_rust_generates_a_new_project,
    WASM,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        assert_all!(dir.path(), ignore: [".git", "target", "Cargo.lock", ".gitignore", "__tmp_bg.wasm"]);
    }
);

decor_test!(
    wasm_for_c_does_not_generate_c_files,
    WASM_C,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        assert_all!(dir.path(), ignore: ["__tmp.js"]);
    }
);

decor_test!(
    wasm_is_integrated_into_index_html,
    WASM_C,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--html");
        cmd.assert().success();
        assert_all!(dir.path(), ignore: ["__tmp.js"]);
    }
);

decor_test!(
    wasm_and_css_are_integrated_into_index_html,
    WASM_C_CSS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--html");
        cmd.assert().success();
        assert_all!(dir.path(), ignore: ["__tmp.js"]);
    }
);

decor_test!(
    zig_files_are_properly_compiled_and_instantiated,
    ZIG,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--html");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    zig_has_imports_exported_objects,
    ZIG_EXPORTS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--html");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    can_run_scss_proprocessor,
    SCSS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    can_override_config,
    SCSS,
    |dir: &mut TempDir, mut cmd: Command| {
        let mut config =
            File::create(dir.path().join("decor.toml")).expect("unable to create config file");

        write!(
            config,
            "preprocessors.scss = {{ pipeline = [\"echo 'span {{ color: red; }}'\"], target = \"css\" }}"
        ).expect("unable to write to config file");

        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    can_merge_configs,
    SCSS_AND_TS,
    |dir: &mut TempDir, mut cmd: Command| {
        let mut config =
            File::create(dir.path().join("decor.toml")).expect("unable to create config file");

        write!(
            config,
            "preprocessors.ts = {{ pipeline = [\"echo x\"], target = \"js\" }}"
        )
        .expect("unable to write to config file");

        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(can_modularize, JS, |dir: &mut TempDir, mut cmd: Command| {
    cmd.args(["--render-method", "csr", "--modularize"]);
    cmd.assert().success();
    assert_all!(dir.path());
});

decor_test!(
    dom_render_is_default_when_modularizing,
    JS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--modularize");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    can_build_go_wasm,
    GO,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        assert_all!(dir.path(), ignore: ["out.wasm", "wasm_exec.js"]);
    }
);

decor_test_multiple!(
    can_strip_binaries,
    WASM_C,
    |no_strip, strip| {
        assert!(strip < no_strip);
    },
    no_strip: |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        let metadata = fs::metadata(dir.path().join("out/__tmp.wasm")).unwrap();
        metadata.len()
    },
    strip: |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--strip");
        cmd.assert().success();
        let metadata = fs::metadata(dir.path().join("out/__tmp.wasm")).unwrap();
        metadata.len()
    }
);

decor_test_multiple!(
    can_optimize_binaries,
    WASM_C,
    |not_optimized, optimized| {
        assert!(optimized < not_optimized);
    },
    not_optimized: |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        let metadata = fs::metadata(dir.path().join("out/__tmp.wasm")).unwrap();
        metadata.len()
    },
    optimized: |dir: &mut TempDir, mut cmd: Command| {
        // Optimize for size
        cmd.arg("-Oz");
        cmd.assert().success();
        let metadata = fs::metadata(dir.path().join("out/__tmp.wasm")).unwrap();
        metadata.len()
    }
);

decor_test_multiple!(
    can_optimize_go_binaries,
    GO,
    |not_optimized, optimized| {
        assert!(optimized < not_optimized);
    },
    not_optimized: |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        let metadata = fs::metadata(dir.path().join("out/out.wasm")).unwrap();
        metadata.len()
    },
    optimized: |dir: &mut TempDir, mut cmd: Command| {
        // Optimize for size
        cmd.arg("-Oz");
        cmd.assert().success();
        let metadata = fs::metadata(dir.path().join("out/out.wasm")).unwrap();
        metadata.len()
    }
);

decor_test!(
    warns_on_unused_strip,
    NO_JS,
    |_dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--strip");
        let assertion = cmd.assert().success();
        insta::assert_snapshot!(String::from_utf8_lossy(
            assertion.get_output().stderr.as_slice()
        ));
    }
);

decor_test!(
    warns_on_unused_optimize,
    NO_JS,
    |_dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("-Oz");
        let assertion = cmd.assert().success();
        insta::assert_snapshot!(String::from_utf8_lossy(
            assertion.get_output().stderr.as_slice()
        ));
    }
);

decor_test!(
    warns_on_unused_build_args,
    NO_JS,
    |_dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("-B rust=\"argument\"");
        let assertion = cmd.assert().success();
        insta::assert_snapshot!(String::from_utf8_lossy(
            assertion.get_output().stderr.as_slice()
        ));
    }
);

decor_test!(
    warn_on_deps_that_are_not_found,
    GO,
    |dir: &mut TempDir, mut cmd: Command| {
        let mut config =
            File::create(dir.path().join("decor.toml")).expect("unable to create config file");
        let go_path = dir.path().join("go.py");
        fs::write(&go_path, "print(\"hello\")").unwrap();

        write!(config, r#"compilers.go = {{ script = "{}", features = [], deps = ["decorshouldneverbefoundonasystem"] }}"#, go_path.to_string_lossy().escape_default()).expect("unable to write to config file");
        let assertion = cmd.assert().success();
        insta::assert_snapshot!(String::from_utf8_lossy(
            assertion.get_output().stderr.as_slice()
        ));
    }
);

decor_test!(
    can_disable_colorization,
    NO_JS,
    |_dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--color=never");
        let assertion = cmd.assert().success();
        let stdout = String::from_utf8_lossy(assertion.get_output().stdout.as_slice());
        let filtered_stdout = stdout
            .lines()
            .filter(|line| !line.starts_with("DONE compiled in"))
            .collect::<Vec<_>>()
            .join("\n");
        insta::assert_snapshot!(filtered_stdout);
    }
);
