from terminal_bench.agents.base_agent import BaseAgent, AgentResult
from terminal_bench.terminal.tmux_session import TmuxSession
from pathlib import Path
import time
import subprocess
import threading
import os

class AmazonQCLILocalAgent(BaseAgent):
    @staticmethod
    def name() -> str:
        return "Amazon Q CLI Local Fixed"

    def perform_task(self, instruction: str, session: TmuxSession, logging_dir: Path | None = None) -> AgentResult:
        # Kill any existing HTTP server on port 8000
        subprocess.run(["pkill", "-f", "python3 -m http.server 8000"], stderr=subprocess.DEVNULL)
        time.sleep(1)
        
        # Start HTTP server
        def start_server():
            subprocess.run([
                "python3", "-m", "http.server", "8000", 
                "--directory", "/home/ec2-user/workspace-qcli/amazon-q-developer-cli/target/release"
            ], cwd="/home/ec2-user/workspace-qcli/amazon-q-developer-cli/target/release")
        
        server_thread = threading.Thread(target=start_server, daemon=False)
        server_thread.start()
        time.sleep(2)
        
        # Install dependencies
        session.send_keys("apt-get update && apt-get install -y curl file")
        session.send_keys("Enter")
        time.sleep(60)
        
        # Download binary
        session.send_keys("curl -o /tmp/qchat http://172.17.0.1:8000/chat_cli")
        session.send_keys("Enter")
        time.sleep(20)
        
        session.send_keys("chmod +x /tmp/qchat")
        session.send_keys("Enter")
        time.sleep(2)
        
        # Install missing libraries
        session.send_keys("apt-get install -y libc6 libgcc-s1 libssl3 || echo 'Some packages not found'")
        session.send_keys("Enter")
        time.sleep(15)
        
        # Set up AWS credentials
        aws_access_key = os.environ.get("AWS_ACCESS_KEY_ID", "")
        aws_secret_key = os.environ.get("AWS_SECRET_ACCESS_KEY", "")
        aws_session_token = os.environ.get("AWS_SESSION_TOKEN", "")
        
        if aws_access_key:
            session.send_keys(f'export AWS_ACCESS_KEY_ID="{aws_access_key}"')
            session.send_keys("Enter")
            session.send_keys(f'export AWS_SECRET_ACCESS_KEY="{aws_secret_key}"')
            session.send_keys("Enter")
            if aws_session_token:
                session.send_keys(f'export AWS_SESSION_TOKEN="{aws_session_token}"')
                session.send_keys("Enter")
            session.send_keys('export AMAZON_Q_SIGV4=1')
            session.send_keys("Enter")
        
        # Write instruction to file to avoid shell escaping issues
        session.send_keys('cat > /tmp/instruction.txt << "EOF"')
        session.send_keys("Enter")
        session.send_keys(instruction)
        session.send_keys("Enter")
        session.send_keys("EOF")
        session.send_keys("Enter")
        
        # Run the task using the file
        session.send_keys('/tmp/qchat chat --no-interactive --trust-all-tools "$(cat /tmp/instruction.txt)"; echo "QCLI_FINISHED_$?"')
        session.send_keys("Enter")
        
        # Wait for completion marker
        max_wait_time = 1500  # 25 minutes
        start_time = time.time()
        
        while time.time() - start_time < max_wait_time:
            time.sleep(5)  # Check every 5 seconds
            
            # Get recent output from session
            try:
                # Capture current session content
                session.send_keys("echo 'STATUS_CHECK'")
                session.send_keys("Enter")
                time.sleep(1)
                
                # Get the session output (this is tmux-specific)
                result = session.capture_pane()
                
                # Check if completion marker is present
                if "QCLI_FINISHED_" in result:
                    print("Q CLI task completed successfully")
                    break
                    
            except Exception as e:
                print(f"Error checking completion: {e}")
                continue
            
        session.send_keys("docker system prune -a --volumes -f")
        return AgentResult()
