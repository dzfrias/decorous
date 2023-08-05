use std::{collections::HashMap, hash::Hash, path::PathBuf};

use merge::Merge;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub python: Option<PathBuf>,

    pub compilers: HashMap<String, CompilerConfig>,
    pub preprocessors: HashMap<String, PreprocessPipeline>,
}

impl Merge for Config {
    fn merge(&mut self, other: Self) {
        self.python.merge(other.python);
        hashmap(&mut self.compilers, other.compilers);
        hashmap(&mut self.preprocessors, other.preprocessors);
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            python: None,
            preprocessors: HashMap::from_iter([(
                "scss".to_owned(),
                PreprocessPipeline {
                    pipeline: vec!["sass --stdin".to_owned()],
                    target: PreprocTarget::Css,
                },
            )]),

            compilers: HashMap::from_iter([
                (
                    "rust".to_owned(),
                    CompilerConfig {
                        ext_override: Some("rs".to_owned()),
                        script: ScriptOrFile::Script(include_str!("./compilers/rust.py")),
                    },
                ),
                (
                    "c++".to_owned(),
                    CompilerConfig {
                        ext_override: Some("cpp".to_owned()),
                        script: ScriptOrFile::Script(include_str!("./compilers/emscripten.py")),
                    },
                ),
                (
                    "c".to_owned(),
                    CompilerConfig {
                        ext_override: None,
                        script: ScriptOrFile::Script(include_str!("./compilers/emscripten.py")),
                    },
                ),
                (
                    "zig".to_owned(),
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
pub struct CompilerConfig {
    pub ext_override: Option<String>,
    #[serde(deserialize_with = "deserialize_script")]
    pub script: ScriptOrFile,
}

#[derive(Debug)]
pub enum ScriptOrFile {
    Script(&'static str),
    File(PathBuf),
}

fn deserialize_script<'de: 'a, 'a, D>(des: D) -> Result<ScriptOrFile, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(ScriptOrFile::File(<PathBuf>::deserialize(des)?))
}

#[derive(Debug, Deserialize)]
pub struct PreprocessPipeline {
    pub pipeline: Vec<String>,
    pub target: PreprocTarget,
}

#[derive(Debug, Deserialize, Clone, Copy, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PreprocTarget {
    Css,
    Js,
}

fn hashmap<K, V>(left: &mut HashMap<K, V>, right: HashMap<K, V>)
where
    K: Eq + Hash,
{
    left.extend(right);
}
