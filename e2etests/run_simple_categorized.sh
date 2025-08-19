#!/bin/bash

# Simple categorized test runner with real-time output and summary
# Usage: ./run_simple_categorized.sh [path_to_q_binary] [--quiet]

# ============================================================================
# CATEGORY CONFIGURATION - Set to true/false to enable/disable categories
# ============================================================================
RUN_CORE_SESSION=true
RUN_AGENT=true
RUN_CONTEXT=true
RUN_SAVE_LOAD=true
RUN_MODEL=true
RUN_SESSION_MGMT=true
RUN_INTEGRATION=true
RUN_MCP=true
RUN_AI_PROMPTS=true
RUN_ISSUE_REPORTING=true
RUN_TOOLS=true
RUN_COMPACT=true
RUN_HOOKS=true
RUN_USAGE=true
RUN_EDITOR=true
RUN_SUBSCRIBE=true
# ============================================================================

Q_BINARY="q"
QUIET_MODE=false

# Parse arguments properly
while [[ $# -gt 0 ]]; do
    case $1 in
        --quiet|-q)
            QUIET_MODE=true
            shift
            ;;
        *)
            if [ "$Q_BINARY" = "q" ]; then
                Q_BINARY="$1"
            fi
            shift
            ;;
    esac
done

if [ "$Q_BINARY" != "q" ]; then
    export Q_CLI_PATH="$Q_BINARY"
fi

echo "🚀 Running Q CLI E2E tests by category"
echo "======================================"

# Initialize counters
total_passed=0
total_failed=0
failed_categories=""

run_category() {
    local category=$1
    local name=$2
    
    echo ""
    echo "🧪 $name"
    echo "$(printf '%*s' ${#name} '' | tr ' ' '-')"
    
    # Show which tests will run in this category
    echo "📋 Tests in this category:"
    for file in tests/*.rs; do
        if grep -q "cfg(feature = \"$category\")" "$file" 2>/dev/null; then
            test_name=$(basename "$file" .rs)
            echo "   • $test_name"
        fi
    done
    echo ""
    
    echo "🔄 Running tests..."
    
    if [ "$QUIET_MODE" = true ]; then
        # Quiet mode - show individual test results in real-time
        cargo test --tests --features "$category" -- --test-threads=1 2>&1 | while IFS= read -r line; do
            if echo "$line" | grep -q "test .* \.\.\. ok$"; then
                test_name=$(echo "$line" | sed 's/test \(.*\) \.\.\. ok/\1/')
                echo "   ✅ $test_name"
            elif echo "$line" | grep -q "test .* \.\.\. FAILED$"; then
                test_name=$(echo "$line" | sed 's/test \(.*\) \.\.\. FAILED/\1/')
                echo "   ❌ $test_name"
            fi
        done
        
        # Check the exit status of cargo test
        if [ ${PIPESTATUS[0]} -eq 0 ]; then
            echo "✅ $name completed successfully"
            return 0
        else
            echo "❌ $name had failures"
            if [ -n "$failed_categories" ]; then
                failed_categories="$failed_categories\n$name"
            else
                failed_categories="$name"
            fi
            return 1
        fi
    else
        # Verbose mode - show full output with real-time test results
        cargo test --tests --features "$category" -- --nocapture --test-threads=1 2>&1 | while IFS= read -r line; do
            echo "$line"
            if echo "$line" | grep -q "test .* \.\.\. ok$"; then
                test_name=$(echo "$line" | sed 's/test \(.*\) \.\.\. ok/\1/')
                echo "   ✅ $test_name PASSED"
            elif echo "$line" | grep -q "test .* \.\.\. FAILED$"; then
                test_name=$(echo "$line" | sed 's/test \(.*\) \.\.\. FAILED/\1/')
                echo "   ❌ $test_name FAILED"
            fi
        done
        
        # Check the exit status of cargo test
        if [ ${PIPESTATUS[0]} -eq 0 ]; then
            echo ""
            echo "✅ $name completed successfully"
            return 0
        else
            echo ""
            echo "❌ $name had failures"
            if [ -n "$failed_categories" ]; then
                failed_categories="$failed_categories\n$name"
            else
                failed_categories="$name"
            fi
            return 1
        fi
    fi
}

# Run each category and track results
if [ "$RUN_CORE_SESSION" = true ]; then
    if run_category "core_session" "Core Session Commands"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_AGENT" = true ]; then
    if run_category "agent" "Agent Commands"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_CONTEXT" = true ]; then
    if run_category "context" "Context Commands"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_SAVE_LOAD" = true ]; then
    if run_category "save_load" "Save/Load Commands"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_MODEL" = true ]; then
    if run_category "model" "Model Commands"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_SESSION_MGMT" = true ]; then
    if run_category "session_mgmt" "Session Management Commands"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_INTEGRATION" = true ]; then
    if run_category "integration" "Integration Commands"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_MCP" = true ]; then
    if run_category "mcp" "MCP Commands"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_AI_PROMPTS" = true ]; then
    if run_category "ai_prompts" "AI Prompts"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_ISSUE_REPORTING" = true ]; then
    if run_category "issue_reporting" "ISSUE REPORTING"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_TOOLS" = true ]; then
    if run_category "tools" "TOOLS"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_COMPACT" = true ]; then
    if run_category "compact" "COMPACT"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_HOOKS" = true ]; then
    if run_category "hooks" "HOOKS"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_USAGE" = true ]; then
    if run_category "usage" "USAGE"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_EDITOR" = true ]; then
    if run_category "editor" "EDITOR"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

if [ "$RUN_SUBSCRIBE" = true ]; then
    if run_category "subscribe" "SUBSCRIBE"; then
        ((total_passed++))
    else
        ((total_failed++))
    fi
fi

# Final summary
echo ""
echo "🎯 FINAL SUMMARY"
echo "================================"
echo "✅ Categories Passed: $total_passed"
echo "❌ Categories Failed: $total_failed"
echo "📊 Total Categories: $((total_passed + total_failed))"

if [ -n "$failed_categories" ]; then
    echo ""
    echo "❌ Failed Categories:"
    echo -e "$failed_categories" | while read -r category; do
        if [ -n "$category" ]; then
            echo "   • $category"
        fi
    done
    echo ""
    echo "💥 Some tests failed!"
    exit 1
else
    echo ""
    echo "🎉 All categories passed!"
    exit 0
fi
