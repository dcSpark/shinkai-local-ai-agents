// embedding_model.rs
// This file handles loading the embedding model from a binary file.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

pub fn get_model_data(llm_models_path: &Path) -> &'static [u8] {
    static FULL_MODEL_DATA: OnceLock<Vec<u8>> = OnceLock::new();

    FULL_MODEL_DATA.get_or_init(|| {
        let model_path = llm_models_path.join("embeddinggemma-300M-BF16.gguf");
        
        let mut file = match File::open(&model_path) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("Failed to open embedding model file at {:?}: {}", model_path, err);
                return Vec::new(); // Return empty vector on error
            }
        };
        
        let mut buffer = Vec::new();
        if let Err(err) = file.read_to_end(&mut buffer) {
            eprintln!("Failed to read embedding model file: {}", err);
            return Vec::new(); // Return empty vector on error
        }
        
        buffer
    }).as_slice()
}

pub const MODEL_NAME: &str = "embeddinggemma:300m";

pub fn get_model_name() -> &'static str {
    MODEL_NAME
}
