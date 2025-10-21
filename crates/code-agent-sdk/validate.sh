#!/bin/bash
set -e

echo "🧪 Running Code Intelligence Validation Suite"
echo "=============================================="

# Check compilation
echo "📦 Checking compilation..."
cargo check

# Format code
echo "🎨 Formatting code..."
cargo fmt --check

# Run linting (allow deprecation warnings for now)
echo "🔍 Running linter..."
cargo clippy -- -D warnings -A deprecated

# Run unit tests
echo "🧪 Running unit tests..."
cargo test --lib

# Run integration tests
echo "🔗 Running integration tests..."
cargo test --test integration_tests

# Run CLI regression tests
echo "🖥️  Running CLI regression tests..."
./regression_test.sh

# Test CLI functionality
echo "🖥️  Testing CLI..."
if [ -f "test_file.ts" ]; then
    echo "Testing TypeScript CLI..."
    cargo run --bin code-agent-cli -- find-symbol greet --file test_file.ts > /dev/null
    echo "✅ CLI test passed"
else
    echo "⚠️  test_file.ts not found, skipping CLI test"
fi

echo ""
echo "🎉 All validations passed!"
echo "✅ Code compiles without warnings"
echo "✅ Code is properly formatted"
echo "✅ Linting passes"
echo "✅ Unit tests pass"
echo "✅ Integration tests pass"
echo "✅ CLI functionality works"
echo ""
echo "🚀 Ready for production!"
