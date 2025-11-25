// TypeScript test file for integration tests
function greet(name: string): string {
    return `Hello, ${name}!`;
}

function main() {
    const message = greet("World");
    console.log(message);
}

class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
}

export { greet, main, Calculator };
