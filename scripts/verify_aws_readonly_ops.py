#!/usr/bin/env python3
"""Verify readonly operations exist in AWS CLI."""

import json
import subprocess
import urllib.request
import re
import random

def to_kebab(name):
    result = []
    for i, c in enumerate(name):
        if c.isupper() and i > 0:
            prev = name[i-1]
            if prev.islower() or prev.isdigit():
                result.append('-')
            elif i + 1 < len(name) and name[i+1].islower():
                result.append('-')
        result.append(c.lower())
    return ''.join(result)

def main():
    print("Fetching service list...")
    with urllib.request.urlopen("http://servicereference.us-east-1.amazonaws.com/") as resp:
        services = json.load(resp)
    
    # Build list of (cli_service, cli_operation) for readonly ops
    readonly_pairs = []
    
    for svc in services[:50]:  # Check first 50 services
        try:
            with urllib.request.urlopen(svc['url']) as resp:
                data = json.load(resp)
            
            action_readonly = {}
            for action in data.get('Actions', []):
                props = action.get('Annotations', {}).get('Properties', {})
                action_readonly[action['Name']] = not props.get('IsWrite', False) and not props.get('IsPermissionManagement', False)
            
            for op in data.get('Operations', []):
                authorized = op.get('AuthorizedActions', [])
                if not authorized:
                    continue
                all_ro = all(action_readonly.get(a['Name'], False) for a in authorized)
                if all_ro:
                    for sdk in op.get('SDK', []):
                        if sdk.get('Package') == 'Boto3':
                            cli_svc = sdk['Name']
                            cli_op = to_kebab(op['Name'])
                            readonly_pairs.append((cli_svc, cli_op))
                            break
        except:
            pass
    
    print(f"Found {len(readonly_pairs)} readonly operations with service mapping")
    
    # Verify random sample
    sample = random.sample(readonly_pairs, min(20, len(readonly_pairs)))
    print(f"\nVerifying {len(sample)} random operations:")
    
    passed = 0
    for svc, op in sample:
        result = subprocess.run(["aws", svc, op, "help"], capture_output=True, timeout=10)
        status = "✓" if result.returncode == 0 else "✗"
        if result.returncode == 0:
            passed += 1
        print(f"  {status} aws {svc} {op}")
    
    print(f"\nPassed: {passed}/{len(sample)}")

if __name__ == "__main__":
    main()
