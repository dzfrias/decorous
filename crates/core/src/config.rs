use std::{collections::HashMap, path::Path};

use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
pub struct Config<'a> {
    pub python: Option<&'a Path>,

    #[serde(borrow)]
    pub compilers: HashMap<&'a str, CompilerConfig<'a>>,
}

impl Default for Config<'_> {
    fn default() -> Self {
        Self {
            python: None,
            compilers: HashMap::from_iter([
                (
                    "rust",
                    CompilerConfig {
                        ext_override: Some("rs"),
                        script: ScriptOrFile::Script(include_str!("./compilers/rust.py")),
                    },
                ),
                (
                    "c++",
                    CompilerConfig {
                        ext_override: Some("cpp"),
                        script: ScriptOrFile::Script(include_str!("./compilers/emscripten.py")),
                    },
                ),
                (
                    "c",
                    CompilerConfig {
                        ext_override: None,
                        script: ScriptOrFile::Script(include_str!("./compilers/emscripten.py")),
                    },
                ),
                (
                    "zig",
                    CompilerConfig {
                        ext_override: None,
                        script: ScriptOrFile::Script(include_str!("./compilers/zig.py")),
                    },
                ),
            ]),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CompilerConfig<'a> {
    pub ext_override: Option<&'a str>,
    #[serde(deserialize_with = "deserialize_script")]
    pub script: ScriptOrFile<'a>,
}

#[derive(Debug)]
pub enum ScriptOrFile<'a> {
    Script(&'static str),
    File(&'a Path),
}

fn deserialize_script<'de: 'a, 'a, D>(des: D) -> Result<ScriptOrFile<'a>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(ScriptOrFile::File(<&Path>::deserialize(des)?))
}
