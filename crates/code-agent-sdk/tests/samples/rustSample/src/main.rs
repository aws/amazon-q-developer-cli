// Simple Rust test file
fn greet_user(name: &str, age: u32) -> String {
    format!("Hello, {}! You are {} years old.", name, age)
}

fn main() {
    let greeting1 = greet_user("Alice", 30);
    println!("{}", greeting1);

    let greeting2 = greet_user("Bob", 25);
    println!("{}", greeting2);

    let result = greet_user("Charlie", 35);
    println!("{}", result);
}
