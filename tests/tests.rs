mod inputs;
mod macros;

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
        cmd.arg("-r").arg("dom");
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
        cmd.arg("-r").arg("dom");
        cmd.assert().success();
        assert_all!(dir.path());
    }
);

decor_test!(
    wasm_for_rust_generates_a_new_project,
    WASM,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        assert_all!(dir.path(), ignore: [".git", "target", "Cargo.lock", ".gitignore", "decor_out_bg.wasm"]);
    }
);

decor_test!(
    wasm_for_c_does_not_generate_c_files,
    WASM_C,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.assert().success();
        assert_all!(dir.path(), ignore: ["input.js"]);
    }
);

decor_test!(
    wasm_is_integrated_into_index_html,
    WASM_C,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--html");
        cmd.assert().success();
        assert_all!(dir.path(), ignore: ["input.js"]);
    }
);

decor_test!(
    wasm_and_css_are_integrated_into_index_html,
    WASM_C_CSS,
    |dir: &mut TempDir, mut cmd: Command| {
        cmd.arg("--html");
        cmd.assert().success();
        assert_all!(dir.path(), ignore: ["input.js"]);
    }
);
