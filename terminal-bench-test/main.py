from terminal_bench.agents.base_agent import BaseAgent, AgentResult
from terminal_bench.terminal.tmux_session import TmuxSession
from pathlib import Path
import time
import os

class AmazonQCLIAgent(BaseAgent):
    @staticmethod
    def name() -> str:
        return "Amazon Q CLI"

    def perform_task(self, instruction: str, session: TmuxSession, logging_dir: Path | None = None) -> AgentResult:
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
        
        # Run the task using qchat from /usr/local/bin (installed by setup script)
        session.send_keys('/usr/local/bin/qchat chat --no-interactive --trust-all-tools "$(cat /tmp/instruction.txt)"; echo "QCLI_FINISHED_$?"')
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
        
        return AgentResult()