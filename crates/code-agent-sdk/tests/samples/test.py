# Python test file for integration tests
def greet(name: str) -> str:
    return f"Hello, {name}!"

def main():
    message = greet("World")
    print(message)

class Calculator:
    def add(self, a: int, b: int) -> int:
        return a + b

if __name__ == "__main__":
    main()
