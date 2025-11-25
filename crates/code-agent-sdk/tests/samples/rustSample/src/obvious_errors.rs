
// This file has VERY obvious errors that should generate diagnostics

pub fn main() {
    // Error 1: Undefined variable
    println!("{}", this_variable_does_not_exist);
    
    // Error 2: Wrong function call
    some_function_that_does_not_exist();
    
    // Error 3: Type mismatch
    let number: i32 = "this is a string not a number";
    
    // Error 4: Unused variable (warning)
    let unused_variable = 42;
    
    // Error 5: Missing semicolon (syntax error)
    let x = 5
    let y = 10;
}

// Error 6: Function with wrong return type
pub fn returns_string() -> String {
    42  // returning number instead of string
}
