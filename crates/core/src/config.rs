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
            preprocessors: HashMap::from_iter([
                (
                    "scss".to_owned(),
                    PreprocessPipeline {
                        pipeline: vec!["sass --stdin".to_owned()],
                        target: PreprocTarget::Css,
                    },
                ),
                (
                    "sass".to_owned(),
                    PreprocessPipeline {
                        pipeline: vec!["sass --stdin --indented".to_owned()],
                        target: PreprocTarget::Css,
                    },
                ),
            ]),

            compilers: HashMap::from_iter([
                (
                    "rust".to_owned(),
                    CompilerConfig {
                        ext_override: Some("rs".to_owned()),
                        script: ScriptOrFile::Script(include_str!("./compilers/rust.py")),
                        features: vec![],
                    },
                ),
                (
                    "c++".to_owned(),
                    CompilerConfig {
                        ext_override: Some("cpp".to_owned()),
                        script: ScriptOrFile::Script(include_str!("./compilers/emscripten.py")),
                        features: vec![],
                    },
                ),
                (
                    "c".to_owned(),
                    CompilerConfig {
                        ext_override: None,
                        script: ScriptOrFile::Script(include_str!("./compilers/emscripten.py")),
                        features: vec![],
                    },
                ),
                (
                    "zig".to_owned(),
                    CompilerConfig {
                        ext_override: None,
                        script: ScriptOrFile::Script(include_str!("./compilers/zig.py")),
                        features: vec![],
                    },
                ),
                (
                    "go".to_owned(),
                    CompilerConfig {
                        ext_override: None,
                        script: ScriptOrFile::Script(include_str!("./compilers/go.py")),
                        features: vec![WasmFeature(wasm_opt::Feature::BulkMemory)],
                    },
                ),
                (
                    "tinygo".to_owned(),
                    CompilerConfig {
                        ext_override: Some("go".to_owned()),
                        script: ScriptOrFile::Script(include_str!("./compilers/tinygo.py")),
                        features: vec![],
                    },
                ),
                (
                    "wat".to_owned(),
                    CompilerConfig {
                        ext_override: None,
                        script: ScriptOrFile::Script(include_str!("./compilers/wat.py")),
                        features: vec![],
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
    pub features: Vec<WasmFeature>,
}

#[derive(Debug)]
pub struct WasmFeature(pub wasm_opt::Feature);

impl<'de> Deserialize<'de> for WasmFeature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use wasm_opt::Feature::*;

        let feature_str = <&str>::deserialize(deserializer)?;
        let feature = match feature_str {
            "atomics" => Atomics,
            "trunc_sat" => TruncSat,
            "simd" => Simd,
            "bulk_memory" => BulkMemory,
            "exception_handling" => ExceptionHandling,
            "tail_call" => TailCall,
            "reference_types" => ReferenceTypes,
            "multivalue" => Multivalue,
            "gc" => Gc,
            "memory64" => Memory64,
            "gc_nn_locals" => GcNnLocals,
            "relaxed_simd" => RelaxedSimd,
            "extended_const" => ExtendedConst,
            "strings" => Strings,
            "multi_memories" => MultiMemories,
            "mvp" => Mvp,
            "all" => All,
            "all_possible" => AllPossible,
            _ => return Err(serde::de::Error::custom("invalid WebAssembly feature")),
        };

        Ok(WasmFeature(feature))
    }
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
