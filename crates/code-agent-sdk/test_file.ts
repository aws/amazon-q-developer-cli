// Simple TypeScript test file
function greet(name: string): string {
    return `Hello, ${name}!`;
}

function main() {
    const message = greet("World");
    console.log(message);
}

export { greet, main };
