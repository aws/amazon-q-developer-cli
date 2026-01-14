#!/usr/bin/env python3
"""
Generate list of readonly AWS CLI operations from official AWS Service Authorization Reference.
Uses CLI service names (s3api, configservice) not SDK names (s3, config).
"""

import json
import re
import urllib.request

# SDK to CLI service name mappings (where they differ)
SDK_TO_CLI = {
    's3': 's3api',
    'config': 'configservice',
    'codedeploy': 'deploy',
}

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
    
    print(f"Processing {len(services)} services...")
    readonly_ops = set()
    
    for i, svc in enumerate(services):
        try:
            with urllib.request.urlopen(svc['url']) as resp:
                data = json.load(resp)
            
            action_readonly = {}
            for action in data.get('Actions', []):
                props = action.get('Annotations', {}).get('Properties', {})
                is_write = props.get('IsWrite', False)
                is_perm_mgmt = props.get('IsPermissionManagement', False)
                action_readonly[action['Name']] = not is_write and not is_perm_mgmt
            
            for op in data.get('Operations', []):
                authorized = op.get('AuthorizedActions', [])
                if not authorized:
                    continue
                
                all_readonly = all(action_readonly.get(a['Name'], False) for a in authorized)
                if all_readonly:
                    for sdk in op.get('SDK', []):
                        if sdk.get('Package') == 'Boto3':
                            sdk_svc = sdk['Name']
                            cli_svc = SDK_TO_CLI.get(sdk_svc, sdk_svc)
                            cli_op = to_kebab(op['Name'])
                            readonly_ops.add(f"{cli_svc}:{cli_op}")
                            break
            
            if (i + 1) % 50 == 0:
                print(f"  Processed {i + 1}/{len(services)}...")
                
        except Exception as e:
            print(f"  Warning: {svc['service']}: {e}")
    
    sorted_ops = sorted(readonly_ops)
    output_path = "crates/chat-cli/src/data/aws_readonly_operations.json"
    with open(output_path, 'w') as f:
        json.dump(sorted_ops, f, indent=2)
    
    print(f"\nWrote {len(sorted_ops)} service:operation pairs to {output_path}")

if __name__ == "__main__":
    main()
