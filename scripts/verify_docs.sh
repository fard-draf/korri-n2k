#!/bin/bash
# Documentation and examples verification script
# Ensures all documented code is up to date and functional

set -e  # Abort on error

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_DIR"

echo "=========================================="
echo "korri-n2k documentation check"
echo "=========================================="
echo ""

# 1. Run doctests (examples in /// comments)
echo "1. Running doctests (inline code examples)..."
cargo test --doc
echo "✓ Doctests OK"
echo ""

# 2. Compile every example
echo "2. Building examples..."
cargo build --examples
echo "✓ Examples built"
echo ""

# 3. Run the quickstart example
echo "3. Running the quickstart example..."
cargo run --example quickstart
echo "✓ Quickstart ran successfully"
echo ""

# 4. Ensure cargo doc builds
echo "4. Generating documentation..."
cargo doc --no-deps --document-private-items
echo "✓ Documentation generated"
echo ""

# 5. Run unit and integration tests
echo "5. Running tests..."
cargo test --lib
echo "✓ Tests OK"
echo ""

echo "=========================================="
echo "✓ All checks passed!"
echo "=========================================="
echo ""
echo "Documentation is up to date and every example works."
