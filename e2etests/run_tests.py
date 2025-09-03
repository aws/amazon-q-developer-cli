#!/usr/bin/env python3

import toml
import subprocess
import sys
import argparse
import json
import time
import platform
import re
import threading
from datetime import datetime
from pathlib import Path

def show_spinner(stop_event):
    """Show rotating spinner animation"""
    spinner = ['|', '/', '-', '\\']
    i = 0
    while not stop_event.is_set():
        print(f"\rExecuting... {spinner[i % len(spinner)]}", end="", flush=True)
        time.sleep(0.1)
        i += 1

def strip_ansi(text):
    """Remove ANSI escape sequences from text"""
    ansi_escape = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
    return ansi_escape.sub('', text)

def parse_features():
    """Parse features from Cargo.toml, handling grouped features correctly"""
    cargo_toml = toml.load("Cargo.toml")
    features = cargo_toml.get("features", {})
    
    # Features to always exclude from individual runs
    excluded_features = {"default", "regression", "sanity"}
    
    # Group features (features that contain other features)
    grouped_features = {}
    grouped_sub_features = set()
    child_features = set()
    
    # First pass: identify grouped features and their sub-features
    for feature_name, feature_deps in features.items():
        if feature_name in excluded_features:
            continue
            
        if isinstance(feature_deps, list) and feature_deps:
            # This is a grouped feature
            grouped_features[feature_name] = feature_deps
            grouped_sub_features.update(feature_deps)
            child_features.update(feature_deps)
    
    # Second pass: identify standalone features (not part of any group)
    standalone_features = []
    for feature_name in features.keys():
        if (feature_name not in excluded_features and 
            feature_name not in grouped_features and 
            feature_name not in grouped_sub_features):
            standalone_features.append(feature_name)
    
    return grouped_features, standalone_features, child_features

# Default test suite - always required for cargo test
DEFAULT_TESTSUITE = "sanity"

def parse_test_results(stdout):
    """Parse individual test results from cargo output with their outputs and descriptions"""
    tests = []
    lines = stdout.split('\n')
    
    # Look for test lines followed by result lines
    for i, line in enumerate(lines):
        clean_line = line.strip()
        
        # Look for test declaration lines
        if clean_line.startswith('test ') and ' ...' in clean_line:
            # Extract test name (everything between 'test ' and ' ... ')
            test_name = clean_line.split(' ... ')[0].replace('test ', '').strip()
            
            # Look ahead for the result (ok/FAILED) in the next few lines
            status = None
            result_line_idx = None
            description = ""
            
            # Check all remaining lines for result
            for j in range(i + 1, len(lines)):
                result_line = lines[j].strip()
                if result_line == 'ok':
                    status = "passed"
                    result_line_idx = j
                    break
                elif result_line == 'FAILED':
                    status = "failed"
                    result_line_idx = j
                    break
            
            # If we found a result, add the test
            if status and test_name:
                # Collect output between test declaration and result
                output_lines = [clean_line]
                if result_line_idx:
                    for k in range(i + 1, result_line_idx + 1):
                        if k < len(lines):
                            line_content = lines[k].strip()
                            output_lines.append(line_content)
                
                # Extract description from the full output
                full_output = '\n'.join(output_lines)
                if "🔍 Testing" in full_output and "| Description:" in full_output:
                    # Find the line with the description
                    for line in output_lines:
                        if "🔍 Testing" in line and "| Description:" in line:
                            description = line.split("| Description:")[1].strip()
                            break
                
                tests.append({
                    "name": test_name,
                    "status": status,
                    "output": strip_ansi('\n'.join(output_lines)),  # Full output
                    "description": description
                })
    
    return tests

def run_single_cargo_test(feature, test_suite, binary_path="q", quiet=False):
    """Run cargo test for a single feature with test suite"""
    feature_str = f"{feature},{test_suite}"
    cmd = ["cargo", "test", "--tests", "--features", feature_str, "--", "--nocapture", "--test-threads=1"]
    
    if not quiet:
        print(f"🔄 Running: {feature} with {test_suite}")
        print(f"Command: {' '.join(cmd)}")
    
    # Start rotating animation
    stop_animation = threading.Event()
    animation_thread = threading.Thread(target=show_spinner, args=(stop_animation,))
    animation_thread.start()
    
    start_time = time.time()
    result = subprocess.run(cmd, capture_output=True, text=True)
    end_time = time.time()
    
    # Stop animation
    stop_animation.set()
    animation_thread.join()
    print("\r", end="")  # Clear spinner line
    
    # Parse individual test results
    individual_tests = parse_test_results(result.stdout)
    
    if not quiet:
        print(result.stdout)
        if result.stderr:
            print(result.stderr)
        
        # Show individual test results
        print(f"\n📋 Individual Test Results for {feature}:")
        if individual_tests:
            for test in individual_tests:
                status_icon = "✅" if test["status"] == "passed" else "❌"
                print(f"  {status_icon} {test['name']} - {test['status']}")
        else:
            print(f"  ⚠️ No individual tests detected (parsing may have failed)")
            print(f"  Debug: Looking for 'test ' lines in output...")
            lines = result.stdout.split('\n')
            test_lines = [line for line in lines if 'test ' in line and ' ... ' in line]
            print(f"  Found {len(test_lines)} potential test lines:")
            for line in test_lines[:3]:  # Show first 3
                print(f"    {repr(line.strip())}")
    
    return {
        "feature": feature,
        "test_suite": test_suite,
        "success": result.returncode == 0,
        "duration": round(end_time - start_time, 2),
        "stdout": strip_ansi(result.stdout),
        "stderr": strip_ansi(result.stderr),
        "command": " ".join(cmd),
        "individual_tests": individual_tests
    }

def validate_features(features):
    """Validate that all features exist in Cargo.toml"""
    grouped_features, standalone_features, child_features = parse_features()
    valid_features = set(grouped_features.keys()) | set(standalone_features) | child_features
    invalid_features = [f for f in features if f not in valid_features and f not in {"sanity", "regression"}]
    if invalid_features:
        print(f"❌ Error: Invalid feature(s): {', '.join(invalid_features)}")
        print(f"Available features: {', '.join(sorted(valid_features))}")
        sys.exit(1)

def get_test_suites_from_features(features):
    """Extract test suites (sanity/regression) from feature list"""
    test_suites = []
    if "sanity" in features:
        test_suites.append("sanity")
    if "regression" in features:
        test_suites.append("regression")
    
    # Check if both sanity and regression are specified
    if len(test_suites) > 1:
        print("❌ Error: Only a single test suite is allowed. Cannot run both 'sanity' and 'regression' together.")
        sys.exit(1)
    
    if not test_suites:
        test_suites = [DEFAULT_TESTSUITE]
    
    return test_suites

def run_tests_with_suites(features, test_suites, binary_path="q", quiet=False):
    """Run tests for each feature with each test suite"""
    results = []
    
    for test_suite in test_suites:
        for feature in features:
            if feature not in {"sanity", "regression"}:
                result = run_single_cargo_test(feature, test_suite, binary_path, quiet)
                results.append(result)
                
                individual_tests = result.get("individual_tests", [])
                passed_count = sum(1 for t in individual_tests if t["status"] == "passed")
                failed_count = sum(1 for t in individual_tests if t["status"] == "failed")
                
                status = "✅" if result["success"] else "❌"
                if individual_tests:
                    print(f"{status} {feature} ({test_suite}) - {result['duration']}s - {passed_count} passed, {failed_count} failed")
                else:
                    print(f"{status} {feature} ({test_suite}) - {result['duration']}s - No individual tests detected")
    
    return results

def get_system_info(binary_path="q"):
    """Get Q binary version and system information"""
    system_info = {
        "os": platform.system(),
        "os_version": platform.version(),
        "platform": platform.platform(),
        "python_version": platform.python_version(),
        "q_binary_path": binary_path
    }
    
    # Try to get Q binary version
    try:
        result = subprocess.run([binary_path, "--version"], capture_output=True, text=True, timeout=10)
        if result.returncode == 0:
            system_info["q_version"] = result.stdout.strip()
        else:
            system_info["q_version"] = "Unable to determine version"
    except Exception as e:
        system_info["q_version"] = f"Error getting version: {str(e)}"
    
    return system_info

def generate_report(results, features, test_suites, binary_path="q"):
    """Generate JSON report and console summary"""
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    system_info = get_system_info(binary_path)
    
    # Create reports directory if it doesn't exist
    reports_dir = Path("reports")
    reports_dir.mkdir(exist_ok=True)
    
    # Calculate summary stats from individual tests
    total_individual_tests = 0
    passed_individual_tests = 0
    failed_individual_tests = 0
    
    # Group by feature with individual test details
    feature_summary = {}
    for result in results:
        feature = result["feature"]
        if feature not in feature_summary:
            feature_summary[feature] = {
                "passed": 0, 
                "failed": 0, 
                "test_suites": [],
                "individual_tests": []
            }
        
        # Count individual tests
        individual_tests = result.get("individual_tests", [])
        feature_passed = sum(1 for t in individual_tests if t["status"] == "passed")
        feature_failed = sum(1 for t in individual_tests if t["status"] == "failed")
        
        feature_summary[feature]["passed"] += feature_passed
        feature_summary[feature]["failed"] += feature_failed
        feature_summary[feature]["test_suites"].append(result["test_suite"])
        feature_summary[feature]["individual_tests"].extend(individual_tests)
        
        total_individual_tests += len(individual_tests)
        passed_individual_tests += feature_passed
        failed_individual_tests += feature_failed
    
    # Create JSON report
    report = {
        "timestamp": timestamp,
        "system_info": system_info,
        "summary": {
            "total_feature_runs": len(results),
            "total_individual_tests": total_individual_tests,
            "passed": passed_individual_tests,
            "failed": failed_individual_tests,
            "success_rate": round((passed_individual_tests / total_individual_tests * 100) if total_individual_tests > 0 else 0, 2)
        },
        "features": feature_summary,
        "detailed_results": results
    }
    
    # Generate filename with features and test suites
    # If running all features (sanity/regression mode), use only test suite names
    grouped_features, standalone_features, _ = parse_features()
    all_available_features = list(grouped_features.keys()) + standalone_features
    
    if set(features) == set(all_available_features):
        # Running all features - use only test suite names
        features_str = "-".join(test_suites)
    else:
        # Running specific features - include feature names
        features_str = "-".join(features[:3]) + ("_more" if len(features) > 3 else "")
        features_str += "_" + "-".join(test_suites)
    
    datetime_str = datetime.now().strftime("%m%d%y%H%M%S")
    filename = reports_dir / f"qcli_test_summary_{features_str}_{datetime_str}.json"
    
    # Save JSON report
    with open(filename, "w") as f:
        json.dump(report, f, indent=2)
    
    report["filename"] = str(filename)
    return report

def generate_html_report(json_filename):
    """Generate HTML report from JSON file using template"""
    with open(json_filename, 'r') as f:
        report = json.load(f)
    
    # Load HTML template
    template_path = Path(__file__).parent / 'html_template.html'
    with open(template_path, 'r') as f:
        html_template = f.read()
    
    # Generate HTML filename in reports directory
    json_path = Path(json_filename)
    html_filename = json_path.with_suffix('.html')
    
    # Calculate stats
    total_features = len(report["features"])
    features_100_pass = sum(1 for stats in report["features"].values() if stats["failed"] == 0)
    features_failed = total_features - features_100_pass
    
    # Get test suites from detailed results
    test_suites = list(set(result["test_suite"] for result in report["detailed_results"]))
    
    # Generate test suites content
    test_suites_content = ""
    for suite in test_suites:
        suite_features = {}
        for result in report["detailed_results"]:
            if result["test_suite"] == suite:
                feature = result["feature"]
                if feature not in suite_features:
                    suite_features[feature] = report["features"][feature]
        
        suite_passed = sum(stats["passed"] for stats in suite_features.values())
        suite_failed = sum(stats["failed"] for stats in suite_features.values())
        suite_rate = round((suite_passed / (suite_passed + suite_failed) * 100) if (suite_passed + suite_failed) > 0 else 0, 2)
        
        suite_failed_class = ' collapsible-failed' if suite_failed > 0 else ''
        test_suites_content += f'<button class="collapsible{suite_failed_class}">🧪 {suite.capitalize()} Test Suite - {suite_rate}% Success Rate ({suite_passed} passed, {suite_failed} failed)</button><div class="content">'
        
        # Add features for this suite
        for feature_name, feature_stats in suite_features.items():
            feature_rate = round((feature_stats["passed"] / (feature_stats["passed"] + feature_stats["failed"]) * 100) if (feature_stats["passed"] + feature_stats["failed"]) > 0 else 0, 2)
            
            # Format feature name: remove underscores and capitalize first letter
            formatted_feature_name = feature_name.replace('_', ' ').title()
            failed_class = ' collapsible-failed' if feature_stats["failed"] > 0 else ''
            test_suites_content += f'<button class="collapsible{failed_class}">📦 {formatted_feature_name} - {feature_rate}% ({feature_stats["passed"]} passed, {feature_stats["failed"]} failed)</button><div class="content">'
            
            # Add individual tests
            individual_tests = feature_stats.get("individual_tests", [])
            for test in individual_tests:
                test_class = "test-passed" if test["status"] == "passed" else "test-failed"
                status_icon = "✅" if test["status"] == "passed" else "❌"
                
                # Convert test name to readable format
                test_name = test['name']
                if '::' in test_name:
                    readable_name = test_name.split('::')[-1]
                    if readable_name.startswith('test_'):
                        readable_name = readable_name[5:]
                    readable_name = ' '.join(word.capitalize() for word in readable_name.split('_'))
                else:
                    readable_name = test_name
                
                test_output = strip_ansi(test.get('output', 'No output captured'))
                test_description = test.get('description', '')
                description_html = f'<p>{test_description}</p>' if test_description else ''
                test_suites_content += f'<div class="test-item {test_class}"><h4>{status_icon} {readable_name}</h4>{description_html}<p><strong>Status:</strong> {test["status"].upper()}</p><button class="collapsible">📄 View Test Output</button><div class="content"><div class="stdout">{test_output}</div></div></div>'
            
            # Add stdout/stderr for this feature
            for result in report["detailed_results"]:
                if result["feature"] == feature_name and result["test_suite"] == suite:
                    stderr_content = f'<div class="stdout" style="border-left-color: #dc3545;"><strong>STDERR:</strong><br>{strip_ansi(result["stderr"])}</div>' if result['stderr'] else ''
                    test_suites_content += f'<button class="collapsible">📄 View Full Command Output</button><div class="content"><p><strong>Command:</strong> {result["command"]}</p><p><strong>Duration:</strong> {result["duration"]}s</p><div class="stdout">{strip_ansi(result["stdout"])}</div>{stderr_content}</div>'
            
            test_suites_content += "</div>"  # Close feature content
        
        test_suites_content += "</div>"  # Close suite content
    
    # Prepare histogram data
    feature_names = list(report['features'].keys())
    feature_total_tests = [stats['passed'] + stats['failed'] for stats in report['features'].values()]
    feature_passed_tests = [stats['passed'] for stats in report['features'].values()]
    
    # Fill template with data
    html_content = html_template.format(
        timestamp=report['timestamp'],
        success_rate=report['summary']['success_rate'],
        total_features=total_features,
        features_100_pass=features_100_pass,
        features_failed=features_failed,
        tests_passed=report['summary']['passed'],
        tests_failed=report['summary']['failed'],
        test_suites_content=test_suites_content,
        platform=report['system_info']['platform'],
        q_binary_info=f"{report['system_info']['q_binary_path']} ({report['system_info']['q_version']})",
        feature_names=json.dumps(feature_names),
        feature_total_tests=json.dumps(feature_total_tests),
        feature_passed_tests=json.dumps(feature_passed_tests),
    )
    
    with open(html_filename, 'w') as f:
        f.write(html_content)
    
    return html_filename

def print_summary(report):
    """Print beautified console summary"""
    # Print system info
    print("\n💻 System Information:")
    print(f"  Platform: {report['system_info']['platform']}")
    print(f"  OS: {report['system_info']['os']} {report['system_info']['os_version']}")
    print(f"  Q Binary: {report['system_info']['q_binary_path']}")
    print(f"  Q Version: {report['system_info']['q_version']}")
    
    print("\n📋 Feature Summary:")
    for feature, stats in report["features"].items():
        status = "✅" if stats["failed"] == 0 else "❌"
        suites = ",".join(set(stats["test_suites"]))
        print(f"  {status} {feature} ({suites}): {stats['passed']} passed, {stats['failed']} failed")
        
        # Show individual test details
        for test in stats["individual_tests"]:
            test_status = "✅" if test["status"] == "passed" else "❌"
            description = test.get('description', '')
            desc_text = f" - {description}" if description else ""
            print(f"    {test_status} {test['name']}{desc_text}")
    
    # Calculate feature-level stats
    total_features = len(report["features"])
    features_100_pass = sum(1 for stats in report["features"].values() if stats["failed"] == 0)
    features_failed = total_features - features_100_pass
    
    print("\n🎯 FINAL SUMMARY")
    print("=" * 32)
    print(f"🏷️  Features Tested: {total_features}")
    # print(f"🔄 Feature Runs: {report['summary']['total_feature_runs']}")
    print(f"✅ Features 100% Pass: {features_100_pass}")
    print(f"❌ Features with Failures: {features_failed}")
    print(f"✅ Individual Tests Passed: {report['summary']['passed']}")
    print(f"❌ Individual Tests Failed: {report['summary']['failed']}")
    print(f"📊 Total Individual Tests: {report['summary']['total_individual_tests']}")
    print(f"📈 Success Rate: {report['summary']['success_rate']}%")
  
    
    if report["summary"]["failed"] == 0:
        print("\n🎉 All tests passed!")
    else:
        print("\n💥 Some tests failed!")
    
    print(f"\n📄 Detailed report saved to: {report['filename']}")
    
    # Generate HTML report
    html_filename = generate_html_report(report['filename'])
    print(f"🌐 HTML report saved to: {html_filename}")

def dev_debug():
    """Debug function to show parsed features"""
    print("🔧 Developer Debug Mode")
    print("=" * 30)
    
    grouped_features, standalone_features, child_features = parse_features()
    
    print("\n📦 Grouped Features:")
    for group, deps in grouped_features.items():
        print(f"  {group} = {deps}")
    
    print("\n🔹 Standalone Features:")
    for feature in standalone_features:
        print(f"  {feature}")
    
    print(f"\n🔸 Sub Features:")
    for feature in sorted(child_features):
        print(f"  {feature}")
    
    print(f"\n📊 Summary:")
    print(f"  Grouped: {len(grouped_features)}")
    print(f"  Standalone: {len(standalone_features)}")
    print(f"  Sub: {len(child_features)}")
    print(f"  Total: {len(grouped_features) + len(standalone_features) + len(child_features)}")

def main():
    parser = argparse.ArgumentParser(
        description="""

Q CLI E2E Test Framework - Python script for comprehensive Amazon Q CLI testing
        
This Python script executes end-to-end tests organized into functional feature categories.
Default test suite is 'sanity' providing core functionality validation.
You can also specify 'regression' suite for extended testing (currently no tests added under regression).
Test execution automatically generates both JSON and HTML reports under the reports directory for detailed analysis.
JSON reports contain raw test data, system info, and execution details for programmatic use.
HTML reports provide visual dashboards with charts, summaries, and formatted test results.
Report filenames follow syntax: q_cli_e2e_report_{features}_{suite}_{timestamp}.json/html
Example sanity reports: q_cli_e2e_report_sanity_082825232555.json, example regression: q_cli_e2e_report_regression_082825232555.html

Additional Features:
  • JSON to HTML conversion: Convert JSON test reports to visual HTML dashboards
  • Feature discovery: Automatically detect available test features and list the available features
  • Multiple test suites: Support for sanity and regression test categories
  • Flexible feature selection: Run individual or grouped features
  • Comprehensive reporting: Generate both JSON and HTML reports with charts

Options:
  -h, --help                    Show this help message and exit
  --features <FEATURES>         Comma-separated list of features (Check example section)
  --binary <BINARY_PATH>        Path to Q-CLI binary. If not provided, script will use default "q" (Q-CLI installed on the system)
  --quiet                       Quiet mode
  --list-features               List all available features (Check example section)
  --json-to-html <JSON_PATH>    Convert JSON report (previously generated by running test) to HTML (Check example section)

Syntax:
  run_tests.py [-h] [--features <FEATURES>] [--binary <BINARY_PATH>] [--quiet] [--list-features] [--json-to-html <JSON_PATH>]

Usage:
  %(prog)s [options]                           # Run tests with default settings
  %(prog)s --features <FEATURES>               # Run specific features
  %(prog)s --list-features                     # List available features
  %(prog)s --json-to-html <JSON_PATH>          # Convert JSON report to HTML (provide JSON file path)""",
        epilog="""Examples:
  # Basic usage
  %(prog)s                                     # Run all tests with default sanity suite
  %(prog)s --features usage                    # Run usage tests with default sanity suite
  %(prog)s --features "usage,agent"            # Run usage+agent tests with default sanity suite
  
  # Test suites
  %(prog)s --features sanity                   # Run all tests with sanity suite
  %(prog)s --features regression               # Run all tests with regression suite
  %(prog)s --features "usage,regression"       # Run usage tests with regression suite

  
  # Multiple features (different ways)
  %(prog)s --features "usage,agent,context"    # Comma-separated features with default sanity suite
  %(prog)s --features usage --features agent   # Multiple --features flags with default sanity suite
  %(prog)s --features core_session             # Run grouped feature (includes help,quit,clear) with default sanity suite
  
  # Binary and output options
  %(prog)s --binary /path/to/q --features usage   # Executes the usage tests on provided q-cli binary instead of installed 
  %(prog)s --quiet --features sanity              # Executes the tests in quiet mode
  
  # Utility commands
  %(prog)s --list-features                     # List all available features
  %(prog)s --json-to-html report.json          # Convert JSON report (previously generated by running test) to HTML
  
  # Advanced examples
  %(prog)s --features "core_session,regression" --binary ./target/release/q
  %(prog)s --features "agent,mcp,sanity" --quiet""",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        usage=argparse.SUPPRESS,
        add_help=False
    )
    # Command options
    parser.add_argument("-h", "--help", action="help", help="show this help message and exit")
    parser.add_argument("--list-features", action="store_true", help="List all available features")
    parser.add_argument("--json-to-html", help="Convert JSON report to HTML (provide JSON file path)", metavar="JSON_PATH")
    
    # For backward compatibility
    parser.add_argument("--features", help="Comma-separated list of features")
    parser.add_argument("--binary", default="q", help="Path to Q CLI binary")
    parser.add_argument("--quiet", action="store_true", help="Quiet mode")
    
    args = parser.parse_args()
    
    if args.list_features:
        dev_debug()
        return
    
    if args.json_to_html:
        html_filename = generate_html_report(args.json_to_html)
        print(f"🌐 HTML report generated: {html_filename}")
        return
    
    if not args.features:
        # Run all features with default test suite
        grouped_features, standalone_features, _ = parse_features()
        all_features = list(grouped_features.keys()) + standalone_features
        test_suites = [DEFAULT_TESTSUITE]
    else:
        # Parse requested features
        requested_features = [f.strip() for f in args.features.split(",")]
        validate_features(requested_features)
        test_suites = get_test_suites_from_features(requested_features)
        
        # Remove test suites from feature list
        features_only = [f for f in requested_features if f not in {"sanity", "regression"}]
        
        if not features_only:
            # Only sanity/regression specified - run all features
            grouped_features, standalone_features, _ = parse_features()
            all_features = list(grouped_features.keys()) + standalone_features
        else:
            all_features = features_only
    
    if not args.quiet:
        print("🧪 Running Q CLI E2E Tests")
        print("=" * 40)
        print(f"Features: {', '.join(all_features)}")
        print(f"Test Suites: {', '.join(test_suites)}")
        print()
    
    # Run tests
    results = run_tests_with_suites(all_features, test_suites, args.binary, args.quiet)
    
    # Generate and display report
    report = generate_report(results, all_features, test_suites, args.binary)
    print_summary(report)
    
    # Exit with appropriate code
    sys.exit(0 if report["summary"]["failed"] == 0 else 1)

if __name__ == "__main__":
    main()