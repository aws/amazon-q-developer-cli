use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{
    Digest,
    Sha256,
};

/// Validator for model files using allowlisted SHA256 hashes
pub struct ModelValidator {
    allowlisted_shas: HashMap<&'static str, Vec<&'static str>>,
}

impl Default for ModelValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelValidator {
    /// Create a new model validator with official allowlisted SHAs
    pub fn new() -> Self {
        let mut allowlisted_shas = HashMap::new();

        // Each file can have multiple valid SHAs (for different versions)
        allowlisted_shas.insert("model.safetensors", vec![
            "53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db",
        ]);

        allowlisted_shas.insert("tokenizer.json", vec![
            "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037",
        ]);

        Self { allowlisted_shas }
    }

    /// Validate a file against allowlisted SHAs, removing it if invalid
    ///
    /// Returns true if the file exists and matches an allowlisted SHA,
    /// false if the file doesn't exist, has an invalid SHA, or is removed.
    pub fn validate_file(&self, file_path: &Path) -> bool {
        if !file_path.exists() {
            return false;
        }

        let filename = match file_path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let valid_shas = match self.allowlisted_shas.get(filename) {
            Some(shas) => shas,
            None => return false,
        };

        let actual_sha = match Self::calculate_sha256(file_path) {
            Ok(sha) => sha,
            Err(_) => return false,
        };

        // Check if actual SHA matches any of the valid SHAs
        if !valid_shas.contains(&actual_sha.as_str()) {
            let _ = std::fs::remove_file(file_path); // Remove invalid file
            return false;
        }

        true
    }

    fn calculate_sha256(file_path: &Path) -> Result<String, std::io::Error> {
        let mut file = File::open(file_path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}
