#!/usr/bin/env python3
import json
import os
import sys
from datetime import datetime
from collections import defaultdict
import argparse

def parse_filename_date(filename):
    """Extract date from filename format: qcli_test_summary_sanity_MMDDYYhhmmss.json"""
    try:
        parts = filename.split('_')
        if len(parts) >= 4:
            date_str = parts[-1].replace('.json', '')
            if len(date_str) == 12:  # MMDDYYhhmmss
                month = date_str[:2]
                day = date_str[2:4]
                year = '20' + date_str[4:6]
                hour = date_str[6:8]
                minute = date_str[8:10]
                second = date_str[10:12]
                return datetime.strptime(f"{year}-{month}-{day} {hour}:{minute}:{second}", "%Y-%m-%d %H:%M:%S")
    except:
        pass
    return None

def analyze_reports(directory_path):
    """Analyze all JSON reports in the directory"""
    reports_data = []
    
    for filename in os.listdir(directory_path):
        if filename.endswith('.json') and 'qcli_test_summary' in filename:
            filepath = os.path.join(directory_path, filename)
            
            try:
                with open(filepath, 'r') as f:
                    data = json.load(f)
                
                # Extract date from filename
                file_date = parse_filename_date(filename)
                if not file_date:
                    continue
                
                # Calculate actual total duration from detailed_results
                total_tests = data.get('summary', {}).get('total_individual_tests', 0)
                duration = 0
                for result in data.get('detailed_results', []):
                    duration += result.get('duration', 0)
                
                report_info = {
                    'filename': filename,
                    'date': file_date,
                    'total_tests': total_tests,
                    'passed': data.get('summary', {}).get('passed', 0),
                    'failed': data.get('summary', {}).get('failed', 0),
                    'success_rate': data.get('summary', {}).get('success_rate', 0),
                    'duration': duration,
                    'features': {}
                }
                
                # Extract feature data with actual durations
                for feature_name, feature_data in data.get('features', {}).items():
                    # Find actual duration for this feature from detailed_results
                    feature_duration = 0
                    for result in data.get('detailed_results', []):
                        if result.get('feature') == feature_name:
                            feature_duration = result.get('duration', 0)
                            break
                    
                    report_info['features'][feature_name] = {
                        'passed': feature_data.get('passed', 0),
                        'failed': feature_data.get('failed', 0),
                        'total': feature_data.get('passed', 0) + feature_data.get('failed', 0),
                        'status': 'Pass' if feature_data.get('failed', 0) == 0 else 'Fail',
                        'duration': feature_duration
                    }
                
                reports_data.append(report_info)
                
            except Exception as e:
                print(f"Error processing {filename}: {e}")
    
    # Sort by date
    reports_data.sort(key=lambda x: x['date'])
    return reports_data

def generate_analytics(reports_data):
    """Generate analytical insights"""
    if not reports_data:
        return {}
    
    # Feature failure analysis
    feature_failures = defaultdict(int)
    feature_totals = defaultdict(int)
    all_features = set()
    
    for report in reports_data:
        for feature_name, feature_data in report['features'].items():
            all_features.add(feature_name)
            feature_totals[feature_name] += 1
            if feature_data['status'] == 'Fail':
                feature_failures[feature_name] += 1
    
    # Calculate failure rates and analysis
    feature_failure_rates = {}
    feature_failure_analysis = {}
    for feature in all_features:
        total_runs = feature_totals[feature]
        failed_runs = feature_failures[feature]
        failure_rate = (failed_runs / total_runs) * 100 if total_runs > 0 else 0
        
        feature_failure_rates[feature] = failure_rate
        feature_failure_analysis[feature] = {
            'failure_rate': failure_rate,
            'total_runs': total_runs,
            'failed_runs': failed_runs
        }
    
    # Feature analytics with duration and max test count (calculate first)
    feature_analytics = {}
    for feature in all_features:
        total_duration = 0
        count = 0
        max_tests = 0
        min_duration = float('inf')
        min_duration_test_count = 0
        latest_test_count = 0
        latest_duration = 0
        feature_durations = []
        
        # Collect all durations for this feature in chronological order
        for report in reports_data:
            if feature in report['features']:
                if 'duration' in report['features'][feature]:
                    duration = report['features'][feature]['duration']
                    test_count = report['features'][feature]['total']
                    feature_durations.append(duration)
                    total_duration += duration
                    count += 1
                    if duration < min_duration:
                        min_duration = duration
                        min_duration_test_count = test_count
                max_tests = max(max_tests, report['features'][feature]['total'])
        
        # Find latest values from most recent report
        for report in reversed(reports_data):
            if feature in report['features']:
                latest_test_count = report['features'][feature]['total']
                latest_duration = report['features'][feature].get('duration', 0)
                break
        
        # Find best time with test count >= latest test count
        best_duration_with_tests = float('inf')
        best_test_count = 0
        for report in reports_data:
            if feature in report['features']:
                test_count = report['features'][feature]['total']
                duration = report['features'][feature].get('duration', 0)
                if test_count >= latest_test_count and duration > 0 and duration < best_duration_with_tests:
                    best_duration_with_tests = duration
                    best_test_count = test_count
        
        # Calculate rolling averages
        avg_last_3 = 0
        avg_last_5 = 0
        if len(feature_durations) >= 3:
            avg_last_3 = sum(feature_durations[-3:]) / 3
        if len(feature_durations) >= 5:
            avg_last_5 = sum(feature_durations[-5:]) / 5
        
        feature_analytics[feature] = {
            'avg_duration': round((total_duration / count) / 60, 2) if count > 0 else 0,
            'best_duration': f"{round(min_duration / 60, 2)} ({min_duration_test_count})" if min_duration != float('inf') else "N/A",
            'best_duration_with_tests': f"{round(best_duration_with_tests / 60, 2)} ({best_test_count})" if best_duration_with_tests != float('inf') else "N/A",
            'max_tests': max_tests,
            'latest_test_count': latest_test_count,
            'latest_duration': round(latest_duration / 60, 2),
            'avg_last_3': round(avg_last_3 / 60, 2) if avg_last_3 > 0 else 0,
            'avg_last_5': round(avg_last_5 / 60, 2) if avg_last_5 > 0 else 0
        }
    
    # Duration analysis with feature breakdown (include all tests)
    duration_changes = []
    for i in range(1, len(reports_data)):
        prev_duration = reports_data[i-1]['duration']
        curr_duration = reports_data[i]['duration']
        change = ((curr_duration - prev_duration) / prev_duration) * 100 if prev_duration > 0 else 0
        
        # Calculate change vs previous 3 and 5 executions average
        change_vs_prev_3 = 0
        prev_3_avg = 0
        if i >= 3:
            prev_3_avg = sum(reports_data[j]['duration'] for j in range(i-3, i)) / 3
            change_vs_prev_3 = ((curr_duration - prev_3_avg) / prev_3_avg) * 100 if prev_3_avg > 0 else 0
        
        change_vs_prev_5 = 0
        prev_5_avg = 0
        if i >= 5:
            prev_5_avg = sum(reports_data[j]['duration'] for j in range(i-5, i)) / 5
            change_vs_prev_5 = ((curr_duration - prev_5_avg) / prev_5_avg) * 100 if prev_5_avg > 0 else 0
        
        # Calculate comparisons with historical data only (up to current date)
        historical_reports = reports_data[:i+1]  # Only include reports up to current index
        overall_avg = sum(report['duration'] for report in historical_reports) / len(historical_reports)
        best_time = min(report['duration'] for report in historical_reports)
        
        # Find best time with current test count from historical data only
        current_test_count = reports_data[i]['total_tests']
        best_with_current = float('inf')
        for report in historical_reports:
            if report['total_tests'] >= current_test_count:
                best_with_current = min(best_with_current, report['duration'])
        best_with_current = best_with_current if best_with_current != float('inf') else curr_duration
        
        change_vs_avg = ((curr_duration - overall_avg) / overall_avg) * 100 if overall_avg > 0 else 0
        change_vs_best = ((curr_duration - best_time) / best_time) * 100 if best_time > 0 else 0
        change_vs_best_current = ((curr_duration - best_with_current) / best_with_current) * 100 if best_with_current > 0 else 0
        
        # Calculate feature breakdown with test count changes
        feature_breakdown = []
        for feature in all_features:
            curr_feat_dur = 0
            curr_test_count = 0
            
            if feature in reports_data[i]['features']:
                curr_feat_dur = reports_data[i]['features'][feature].get('duration', 0) / 60
                curr_test_count = reports_data[i]['features'][feature].get('total', 0)
            
            # Find nearest previous execution of this feature
            prev_test_count = 0
            for j in range(i-1, -1, -1):  # Go backwards from current report
                if feature in reports_data[j]['features']:
                    prev_test_count = reports_data[j]['features'][feature].get('total', 0)
                    break
            
            avg_feat_dur = feature_analytics.get(feature, {}).get('avg_duration', 0)
            test_change = curr_test_count - prev_test_count
            
            feature_breakdown.append({
                'feature': feature,
                'current_duration': round(curr_feat_dur, 2),
                'average_duration': avg_feat_dur,
                'current_test_count': curr_test_count,
                'previous_test_count': prev_test_count,
                'test_change': test_change
            })
        
        duration_changes.append({
            'date': reports_data[i]['date'],
            'change_percent': change,
            'change_vs_prev_3': change_vs_prev_3,
            'change_vs_prev_5': change_vs_prev_5,
            'change_vs_avg': change_vs_avg,
            'change_vs_best': change_vs_best,
            'change_vs_best_current': change_vs_best_current,
            'prev_duration_minutes': round(prev_duration / 60, 2),
            'prev_3_avg_minutes': round(prev_3_avg / 60, 2) if prev_3_avg > 0 else 0,
            'prev_5_avg_minutes': round(prev_5_avg / 60, 2) if prev_5_avg > 0 else 0,
            'overall_avg_minutes': round(overall_avg / 60, 2),
            'best_time_minutes': round(best_time / 60, 2),
            'best_current_minutes': round(best_with_current / 60, 2),
            'curr_duration_minutes': round(curr_duration / 60, 2),
            'total_tests': reports_data[i]['total_tests'],
            'significant_test': f"Duration changed by {change:+.1f}%",
            'feature_breakdown': feature_breakdown
        })
    
    # Sort by date descending (most recent first)
    duration_changes.sort(key=lambda x: x['date'], reverse=True)
    
    # Keep backward compatibility
    feature_avg_duration = {k: v['avg_duration'] for k, v in feature_analytics.items()}
    
    return {
        'feature_failure_rates': dict(sorted(feature_failure_rates.items(), key=lambda x: x[1], reverse=True)),
        'feature_failure_analysis': feature_failure_analysis,
        'all_duration_changes': duration_changes,
        'feature_avg_duration': feature_avg_duration,
        'feature_analytics': feature_analytics,
        'all_features': sorted(all_features),
        'total_reports': len(reports_data)
    }

def generate_html_report(reports_data, analytics, output_file):
    """Generate HTML report"""
    
    # Prepare data for charts (convert actual duration to minutes)
    dates = [report['date'].strftime('%m/%d/%y') for report in reports_data]
    durations = [round(report['duration'] / 60, 2) for report in reports_data]  # Actual duration in minutes
    test_counts = [report['total_tests'] for report in reports_data]
    
    # Calculate overall execution statistics
    avg_duration_minutes = sum(durations) / len(durations) if durations else 0
    best_overall_time = min(durations) if durations else 0
    latest_execution_time = durations[-1] if durations else 0
    
    # Calculate best time with latest test count
    latest_test_count = reports_data[-1]['total_tests'] if reports_data else 0
    best_with_current_tests = float('inf')
    for i, report in enumerate(reports_data):
        if report['total_tests'] >= latest_test_count:
            best_with_current_tests = min(best_with_current_tests, durations[i])
    best_with_current_tests = best_with_current_tests if best_with_current_tests != float('inf') else 0
    
    # Calculate rolling averages
    avg_last_3 = sum(durations[-3:]) / len(durations[-3:]) if len(durations) >= 3 else 0
    avg_last_5 = sum(durations[-5:]) / len(durations[-5:]) if len(durations) >= 5 else 0
    
    # Add overall stats to analytics
    overall_stats = {
        'avg_duration': round(avg_duration_minutes, 2),
        'best_time': round(best_overall_time, 2),
        'best_with_current_tests': round(best_with_current_tests, 2),
        'latest_execution': round(latest_execution_time, 2),
        'avg_last_3': round(avg_last_3, 2),
        'avg_last_5': round(avg_last_5, 2)
    }
    
    # Prepare test summary data with average comparison
    test_summary = []
    for report in reports_data:
        duration_minutes = round(report['duration'] / 60, 2)
        diff_from_avg = ((duration_minutes - avg_duration_minutes) / avg_duration_minutes) * 100 if avg_duration_minutes > 0 else 0
        is_significant = abs(diff_from_avg) > 20  # 20% threshold
        
        test_summary.append({
            'date': report['date'].strftime('%m/%d/%y'),
            'total_tests': report['total_tests'],
            'duration_minutes': duration_minutes,
            'duration_hours': round(report['duration'] / 3600, 2),
            'avg_comparison': round(diff_from_avg, 1),
            'is_significant': is_significant
        })
    
    # Feature matrix data
    feature_matrix = []
    for feature in analytics['all_features']:
        row = {'feature': feature}
        for report in reports_data:
            date_key = report['date'].strftime('%m/%d/%y')
            if feature in report['features']:
                row[date_key] = report['features'][feature]['status']
            else:
                row[date_key] = 'N/A'
        feature_matrix.append(row)
    
    with open('analysis_report_template.html', 'r') as f:
        template = f.read()
    
    # Prepare feature-wise trends data
    feature_trends = {}
    for feature in analytics['all_features']:
        feature_durations = []
        feature_test_counts = []
        for report in reports_data:
            if feature in report['features']:
                feature_durations.append(round(report['features'][feature].get('duration', 0) / 60, 2))
                feature_test_counts.append(report['features'][feature]['total'])
            else:
                feature_durations.append(0)
                feature_test_counts.append(0)
        feature_trends[feature] = {
            'durations': feature_durations,
            'test_counts': feature_test_counts
        }
    
    # Replace placeholders
    html_content = template.replace('{{DATES}}', str(dates))
    html_content = html_content.replace('{{DURATIONS}}', str(durations))
    html_content = html_content.replace('{{TEST_COUNTS}}', str(test_counts))
    html_content = html_content.replace('{{FEATURE_MATRIX}}', json.dumps(feature_matrix))
    # Add overall stats to analytics
    analytics['overall_stats'] = overall_stats
    html_content = html_content.replace('{{ANALYTICS}}', json.dumps(analytics, default=str))
    html_content = html_content.replace('{{TEST_SUMMARY}}', json.dumps(test_summary))
    html_content = html_content.replace('{{FEATURE_TRENDS}}', json.dumps(feature_trends))
    
    with open(output_file, 'w') as f:
        f.write(html_content)

def main():
    parser = argparse.ArgumentParser(description='Analyze Amazon Q CLI test reports')
    parser.add_argument('directory', help='Directory containing JSON report files')
    parser.add_argument('-o', '--output', help='Output HTML file (default: timestamped file in report-analysis/)')
    
    args = parser.parse_args()
    
    # Generate timestamped filename if not provided
    if not args.output:
        timestamp = datetime.now().strftime('%m%d%y%H%M%S')
        args.output = f'report-analysis/test_analysis_report_{timestamp}.html'
    
    if not os.path.isdir(args.directory):
        print(f"Error: Directory '{args.directory}' does not exist")
        sys.exit(1)
    
    print("Analyzing test reports...")
    reports_data = analyze_reports(args.directory)
    
    if not reports_data:
        print("No valid test reports found")
        sys.exit(1)
    
    print(f"Found {len(reports_data)} reports")
    
    analytics = generate_analytics(reports_data)
    
    print("Generating HTML report...")
    generate_html_report(reports_data, analytics, args.output)
    
    print(f"Report generated: {args.output}")
    
    # Print summary
    print("\n=== SUMMARY ===")
    print(f"Total Reports: {analytics['total_reports']}")
    print(f"Features with highest failure rates:")
    for feature, rate in list(analytics['feature_failure_rates'].items())[:5]:
        print(f"  {feature}: {rate:.1f}%")
    
    if analytics['all_duration_changes']:
        print(f"\nRecent duration changes:")
        for change in analytics['all_duration_changes'][:3]:  # Show first 3 (most recent)
            print(f"  {change['date'].strftime('%m/%d/%y')}: {change['change_percent']:+.1f}% ({change['curr_duration_minutes']} min)")

if __name__ == "__main__":
    main()