use std::path::PathBuf;
use anyhow::Result;

/// E2E test configuration
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub temp_dir: PathBuf,
    pub timeout_secs: u64,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            temp_dir: std::env::temp_dir().join("code_agent_sdk_e2e"),
            timeout_secs: 30,
        }
    }
}

/// Language-specific test project configuration
#[derive(Debug, Clone)]
pub struct ProjectConfig {
    pub name: String,
    pub language: String,
    pub main_file: String,
    pub source_content: String,
    pub project_files: Vec<(String, String)>, // (filename, content)
}

impl ProjectConfig {
    pub fn rust_project() -> Self {
        Self {
            name: "test_rust".to_string(),
            language: "rust".to_string(),
            main_file: "src/main.rs".to_string(),
            source_content: r#"pub fn greet_user(name: &str, age: u32) -> String {
    format!("Hello, {}! You are {} years old.", name, age)
}

pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let result = greet_user("Alice", 30);
    println!("{}", result);
    println!("Sum: {}", calculate_sum(5, 3));
}
"#.to_string(),
            project_files: vec![
                ("Cargo.toml".to_string(), r#"[package]
name = "test_rust"
version = "0.1.0"
edition = "2021"
"#.to_string()),
            ],
        }
    }

    pub fn typescript_project() -> Self {
        Self {
            name: "test_typescript".to_string(),
            language: "typescript".to_string(),
            main_file: "src/main.ts".to_string(),
            source_content: r#"export function greetUser(name: string, age: number): string {
    return `Hello, ${name}! You are ${age} years old.`;
}

export function calculateSum(a: number, b: number): number {
    return a + b;
}

function main() {
    const result = greetUser("Alice", 30);
    console.log(result);
    console.log(`Sum: ${calculateSum(5, 3)}`);
}

main();
"#.to_string(),
            project_files: vec![
                ("package.json".to_string(), r#"{
  "name": "test_typescript",
  "version": "1.0.0",
  "main": "src/main.ts",
  "scripts": {
    "build": "tsc"
  },
  "devDependencies": {
    "typescript": "^5.0.0"
  }
}
"#.to_string()),
                ("tsconfig.json".to_string(), r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "outDir": "./dist",
    "rootDir": "./src",
    "strict": true,
    "esModuleInterop": true
  }
}
"#.to_string()),
            ],
        }
    }

    pub fn python_project() -> Self {
        Self {
            name: "test_python".to_string(),
            language: "python".to_string(),
            main_file: "main.py".to_string(),
            source_content: r#"def greet_user(name: str, age: int) -> str:
    return f"Hello, {name}! You are {age} years old."

def calculate_sum(a: int, b: int) -> int:
    return a + b

def main():
    result = greet_user("Alice", 30)
    print(result)
    print(f"Sum: {calculate_sum(5, 3)}")

if __name__ == "__main__":
    main()
"#.to_string(),
            project_files: vec![],
        }
    }
}

/// Test project manager
pub struct TestProject {
    pub path: PathBuf,
    pub config: ProjectConfig,
}

impl TestProject {
    pub fn create(config: ProjectConfig, base_path: &PathBuf) -> Result<Self> {
        let project_path = base_path.join(&config.name);
        std::fs::create_dir_all(&project_path)?;

        // Create main file directory if needed
        let main_file_path = project_path.join(&config.main_file);
        if let Some(parent) = main_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write main source file
        std::fs::write(&main_file_path, &config.source_content)?;

        // Write additional project files
        for (filename, content) in &config.project_files {
            let file_path = project_path.join(filename);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file_path, content)?;
        }

        Ok(Self {
            path: project_path,
            config,
        })
    }

    pub fn main_file_path(&self) -> PathBuf {
        self.path.join(&self.config.main_file)
    }
}
