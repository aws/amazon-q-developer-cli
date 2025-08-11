#!/usr/bin/env python3
"""
Redux CLI - Enterprise wrapper for Amazon Q CLI with custom model support
Supports custom model format: custom:<region>:<service>:<model-id>:<version>
Example: custom:us-east-1:anthropic:claude-3-5-sonnet-20241022-v2:0
"""

import os
import sys
import json
import uuid
import subprocess
from datetime import datetime
from pathlib import Path
import argparse
import sqlite3
from typing import Dict, List, Optional

class ReduxCLI:
    def __init__(self):
        self.conversations_dir = os.environ.get(
            'REDUX_CONVERSATIONS_DIR',
            os.path.expanduser('~/.amazon-q/conversations')
        )
        Path(self.conversations_dir).mkdir(parents=True, exist_ok=True)
        self.db_path = os.path.expanduser('~/.amazon-q/db')
        
    def parse_custom_model(self, model_id: str) -> Optional[Dict[str, str]]:
        """Parse custom model format: custom:<region>:<actual-model-id>
        Example: custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0
        """
        if not model_id.startswith('custom:'):
            return None
            
        # Remove "custom:" prefix
        without_prefix = model_id[7:]
        
        # Find first colon to separate region from model ID
        colon_pos = without_prefix.find(':')
        if colon_pos == -1:
            print(f"Error: Invalid custom model format. Expected: custom:<region>:<actual-model-id>")
            return None
            
        region = without_prefix[:colon_pos]
        actual_model_id = without_prefix[colon_pos + 1:]
        
        return {
            'region': region,
            'actual_model_id': actual_model_id,
            'full_id': model_id
        }
    
    def setup_environment(self, model_id: Optional[str] = None):
        """Setup environment variables for custom model support"""
        if model_id and model_id.startswith('custom:'):
            model_info = self.parse_custom_model(model_id)
            if model_info:
                # Enable SigV4 authentication for custom models
                os.environ['AMAZON_Q_SIGV4'] = '1'
                os.environ['AMAZON_Q_CUSTOM_MODEL'] = '1'
                
                # Set AWS region if specified
                if model_info['region']:
                    os.environ['AWS_REGION'] = model_info['region']
                    
                print(f"✓ Configured custom model: {model_info['actual_model_id']}")
                print(f"  Region: {model_info['region']}")
                print(f"  Using AWS credentials chain (no Builder ID required)")
                return model_info
        return None
    
    def get_conversation_from_db(self, conv_id: str) -> Optional[Dict]:
        """Extract conversation from SQLite database"""
        if not os.path.exists(self.db_path):
            return None
            
        try:
            conn = sqlite3.connect(self.db_path)
            cursor = conn.cursor()
            
            # Get conversation metadata
            cursor.execute("""
                SELECT conversation_id, created_at, updated_at 
                FROM conversations 
                WHERE conversation_id = ?
            """, (conv_id,))
            
            conv_data = cursor.fetchone()
            if not conv_data:
                return None
                
            # Get messages
            cursor.execute("""
                SELECT role, content, timestamp, model
                FROM messages 
                WHERE conversation_id = ?
                ORDER BY timestamp
            """, (conv_id,))
            
            messages = []
            for row in cursor.fetchall():
                messages.append({
                    'role': row[0],
                    'content': row[1],
                    'timestamp': row[2],
                    'model': row[3] if row[3] else None
                })
            
            conn.close()
            
            return {
                'conversation_id': conv_data[0],
                'created_at': conv_data[1],
                'updated_at': conv_data[2],
                'messages': messages
            }
            
        except Exception as e:
            print(f"Warning: Could not read from database: {e}")
            return None
    
    def save_conversation_json(self, conv_id: str, model_info: Optional[Dict] = None):
        """Save conversation to JSON file"""
        timestamp = datetime.utcnow().strftime('%Y%m%d_%H%M%S')
        json_file = Path(self.conversations_dir) / f"{conv_id}_{timestamp}.json"
        
        conversation = self.get_conversation_from_db(conv_id)
        
        if not conversation:
            conversation = {
                'conversation_id': conv_id,
                'created_at': datetime.utcnow().isoformat(),
                'messages': []
            }
        
        # Add model information if custom model was used
        if model_info:
            conversation['model_info'] = model_info
            
        # Add AWS metadata
        conversation['metadata'] = {
            'aws_profile': os.environ.get('AWS_PROFILE'),
            'aws_region': os.environ.get('AWS_REGION'),
            'aws_account_id': self.get_aws_account_id()
        }
        
        # Save to JSON
        with open(json_file, 'w') as f:
            json.dump(conversation, f, indent=2, default=str)
            
        print(f"\n✓ Conversation saved to: {json_file}")
        return json_file
    
    def get_aws_account_id(self) -> Optional[str]:
        """Get AWS account ID using STS"""
        try:
            result = subprocess.run(
                ['aws', 'sts', 'get-caller-identity', '--query', 'Account', '--output', 'text'],
                capture_output=True,
                text=True,
                timeout=5
            )
            if result.returncode == 0:
                return result.stdout.strip()
        except:
            pass
        return None
    
    def run(self, args: List[str]):
        """Run the Amazon Q CLI with custom model support"""
        parser = argparse.ArgumentParser(description='Redux CLI - Enterprise Amazon Q wrapper')
        parser.add_argument('--model', '-m', help='Model ID (custom:<region>:<service>:<model>:<version>)')
        parser.add_argument('--conversation-id', '-c', help='Resume or specify conversation ID')
        parser.add_argument('--resume', '-r', action='store_true', help='Resume previous conversation')
        parser.add_argument('--non-interactive', '-n', action='store_true', help='Non-interactive mode')
        parser.add_argument('prompt', nargs='*', help='Initial prompt')
        
        # Parse known args
        known_args, remaining = parser.parse_known_args(args)
        
        # Setup environment for custom model
        model_info = None
        if known_args.model:
            model_info = self.setup_environment(known_args.model)
            if not model_info and known_args.model.startswith('custom:'):
                print("Error: Invalid custom model format")
                sys.exit(1)
        
        # Generate or use conversation ID
        conv_id = known_args.conversation_id or str(uuid.uuid4())
        
        # Build Q CLI command - try to find the binary
        q_binary = 'q'
        
        # Check common locations for the Q CLI binary
        import shutil
        if shutil.which('q'):
            q_binary = 'q'
        elif os.path.exists('/usr/local/bin/q'):
            q_binary = '/usr/local/bin/q'
        elif os.path.exists(os.path.expanduser('~/bin/q')):
            q_binary = os.path.expanduser('~/bin/q')
        else:
            # Try to use the built binary from the project
            script_dir = Path(__file__).parent
            project_root = script_dir.parent
            
            # Check for release build first
            release_binary = project_root / 'target' / 'release' / 'chat_cli'
            debug_binary = project_root / 'target' / 'debug' / 'chat_cli'
            
            if release_binary.exists():
                q_binary = str(release_binary)
            elif debug_binary.exists():
                q_binary = str(debug_binary)
            else:
                print("Error: Could not find 'q' or 'chat_cli' binary")
                print("Please ensure Amazon Q CLI is installed or built")
                sys.exit(1)
        
        q_cmd = [q_binary, 'chat']
        
        if known_args.model:
            q_cmd.extend(['--model', known_args.model])
            
        if known_args.resume:
            q_cmd.append('--resume')
            
        if known_args.non_interactive:
            q_cmd.append('--non-interactive')
            
        # Add prompt if provided
        if known_args.prompt:
            q_cmd.append(' '.join(known_args.prompt))
            
        # Add any remaining args
        q_cmd.extend(remaining)
        
        print(f"Starting conversation: {conv_id}")
        print("-" * 50)
        
        try:
            # Run the Q CLI
            result = subprocess.run(q_cmd)
            
            # Save conversation to JSON after completion
            if model_info or os.environ.get('REDUX_SAVE_CONVERSATIONS') == '1':
                self.save_conversation_json(conv_id, model_info)
                
            return result.returncode
            
        except KeyboardInterrupt:
            print("\n\nConversation interrupted")
            # Still try to save what we have
            if model_info:
                self.save_conversation_json(conv_id, model_info)
            return 130
        except Exception as e:
            print(f"Error: {e}")
            return 1

def main():
    """Main entry point"""
    cli = ReduxCLI()
    sys.exit(cli.run(sys.argv[1:]))

if __name__ == '__main__':
    main()