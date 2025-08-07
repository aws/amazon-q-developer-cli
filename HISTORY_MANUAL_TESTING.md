# Manual Testing Guide for Q CLI History Feature

## Overview
This guide provides comprehensive testing steps for the history feature implementation. For basic usage instructions, see [HISTORY_FEATURE_README.md](./HISTORY_FEATURE_README.md).

## Prerequisites
1. Follow the setup instructions in HISTORY_FEATURE_README.md
2. Have the `qdev` command available (or your chosen method)
3. Have some existing conversations in your database

## Comprehensive Test Cases

### Edge Cases and Error Handling

#### Invalid Input Testing
```bash
# Test with non-existent conversation ID
qdev history show nonexistent
# Expected: "Conversation with ID 'nonexistent' not found"

# Test with empty search query
qdev history search ""
# Expected: Should handle gracefully

# Test with zero limit
qdev history list --limit 0
# Expected: Should show no results or handle gracefully

# Test with very large limit
qdev history list --limit 999999
# Expected: Should not crash, shows available conversations

# Test with special characters in path filter
qdev history list --path "~/test with spaces"
qdev history list --path "path/with/unicode/🚀"
# Expected: Should filter correctly or handle gracefully
```

#### Export Edge Cases
```bash
# Test export to invalid path
qdev history export <valid-id> --output /invalid/path/file.json
# Expected: Clear error message about path

# Test export with invalid format
qdev history export <valid-id> --output test.json --format invalid
# Expected: Error about invalid format

# Test export without write permissions
sudo touch /tmp/readonly.json && sudo chmod 444 /tmp/readonly.json
qdev history export <valid-id> --output /tmp/readonly.json
# Expected: Permission error
```

### Performance Testing

#### Large Database Testing
```bash
# If you have many conversations (50+):
time qdev history list --limit 100
# Expected: Should complete in reasonable time (<2 seconds)

time qdev history search "common term"
# Expected: Should complete in reasonable time

# Test memory usage with large results
qdev history list --limit 1000
# Expected: Should not consume excessive memory
```

### Integration Testing

#### Database Integrity
```bash
# Verify conversations persist after history operations
qdev chat  # Have a conversation
qdev history list  # Should show the new conversation
qdev chat --resume  # Should resume properly
```

#### Cross-Directory Testing
```bash
# Test in multiple directories
mkdir -p /tmp/test1 /tmp/test2
cd /tmp/test1 && qdev chat  # Have conversation 1
cd /tmp/test2 && qdev chat  # Have conversation 2

# Test filtering works
qdev history list --path test1  # Should show only conversation 1
qdev history list --path test2  # Should show only conversation 2
qdev history list --path /tmp   # Should show both
```

#### Export/Import Workflow
```bash
# Full export/import cycle
qdev history export <id> --output test.json
# In a chat session: /load test.json
# Expected: Conversation should load properly

# Test different formats maintain data integrity
qdev history export <id> --output test.md --format markdown
# Expected: Should contain all conversation data in readable format
```

### Regression Testing

#### Ensure Existing Functionality
```bash
# Verify core Q CLI still works
qdev chat  # Should start normally
qdev --help  # Should show all commands including history
qdev whoami  # Should work if logged in

# Verify history doesn't interfere with normal operations
qdev chat  # Have a conversation
# Exit and restart
qdev chat --resume  # Should resume properly
```

### Security and Privacy Testing

#### Data Exposure
```bash
# Verify no sensitive data in error messages
qdev history show invalid-id-with-sensitive-info
# Expected: Generic error, no data exposure

# Check exported files don't contain unexpected data
qdev history export <id> --output test.json
grep -i "password\|token\|secret" test.json
# Expected: No sensitive data found
```

## Visual Verification Checklist

### Table Formatting
- [ ] Columns are properly aligned
- [ ] Unicode table characters display correctly
- [ ] Long directory paths are truncated appropriately
- [ ] Dates are in consistent format
- [ ] Preview text is truncated at reasonable length

### Error Messages
- [ ] Error messages are helpful and actionable
- [ ] No stack traces or debug info in user-facing errors
- [ ] Consistent error message formatting

### Help Text
- [ ] All commands have proper help text
- [ ] Examples in help are accurate
- [ ] Options are clearly documented

## Performance Benchmarks

### Expected Performance
- `qdev history list`: < 1 second for 100 conversations
- `qdev history search`: < 2 seconds for 100 conversations
- `qdev history show`: < 0.5 seconds
- `qdev history export`: < 3 seconds for large conversations

### Memory Usage
- Should not consume > 100MB for normal operations
- Should not have memory leaks during repeated operations

## Stress Testing

#### Rapid Operations
```bash
# Test rapid successive calls
for i in {1..10}; do qdev history list --limit 1; done
# Expected: Should handle without issues

# Test concurrent access (if applicable)
qdev history list & qdev history search "test" & wait
# Expected: Should handle gracefully
```

## Error Recovery Testing

#### Database Issues
```bash
# Test with database locked (simulate)
# Test with corrupted database entries
# Test with missing database file
# Expected: Graceful error handling, no crashes
```

## Cleanup and Maintenance

#### Test Data Management
```bash
# After extensive testing, you may want to clean up
# Note: Currently no built-in cleanup command
# Consider backing up ~/.aws/amazonq/ before extensive testing
```

## Reporting Issues

When reporting bugs, include:
1. **Exact command used**
2. **Full error message or unexpected output**
3. **Steps to reproduce**
4. **Environment details** (OS, shell, Q CLI version)
5. **Database state** (number of conversations, etc.)

## Success Criteria

✅ **All tests should:**
- Complete without crashes
- Provide appropriate error messages for invalid input
- Maintain data integrity
- Perform within acceptable time limits
- Display properly formatted output
- Not interfere with existing Q CLI functionality

❌ **Red flags:**
- Segmentation faults or crashes
- Data corruption or loss
- Extremely slow performance (>10 seconds for basic operations)
- Sensitive data exposure
- Breaking existing Q CLI commands
