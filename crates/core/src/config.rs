use std::{collections::HashMap, path::Path};

use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
pub struct Config<'a> {
    #[serde(borrow)]
    pub compilers: HashMap<&'a str, CompilerConfig<'a>>,
}

impl Default for Config<'_> {
    fn default() -> Self {
        Self {
            compilers: HashMap::from_iter([
                (
                    "rust",
                    CompilerConfig {
                        ext_override: Some("rs"),
                        script: ScriptOrFile::Script(include_str!("./compilers/rust")),
                        group: true,
                    },
                ),
                (
                    "c++",
                    CompilerConfig {
                        ext_override: Some("cpp"),
                        script: ScriptOrFile::Script(include_str!("./compilers/emscripten")),
                        group: false,
                    },
                ),
                (
                    "c",
                    CompilerConfig {
                        ext_override: None,
                        script: ScriptOrFile::Script(include_str!("./compilers/emscripten")),
                        group: false,
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
    pub group: bool,
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
