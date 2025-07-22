import os
import shlex
from pathlib import Path

from terminal_bench.agents.installed_agents.abstract_installed_agent import (
    AbstractInstalledAgent,
)
from terminal_bench.terminal.models import TerminalCommand


class AmazonQCLIAgent(AbstractInstalledAgent):

    @staticmethod
    def name() -> str:
        return "Amazon Q CLI"

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

    """
    Makes necessary env vars available in docker containers
    """
    @property
    def _env(self) -> dict[str, str]:
        # SIGv4 = 1 for AWS credentials
        env = {
            "AMAZON_Q_SIGV4": 1,
            "AWS_ACCESS_KEY_ID": os.environ.get("AWS_ACCESS_KEY_ID", ''),
            "AWS_SECRET_ACCESS_KEY": os.environ.get("AWS_SECRET_ACCESS_KEY", ''),
            "AWS_SESSION_TOKEN": os.environ.get("AWS_SESSION_TOKEN", ''),
            "GIT_HASH": os.environ.get("GIT_HASH", ''),
            "CHAT_DOWNLOAD_ROLE_ARN": os.environ.get("CHAT_DOWNLOAD_ROLE_ARN", ''),
            "CHAT_BUILD_BUCKET_NAME": os.environ.get("CHAT_BUILD_BUCKET_NAME", '')
        }
        return env

    @property
    def _install_agent_script_path(self) -> os.PathLike:
        return Path(__file__).parent / "setup_amazon_q.sh"

    def _run_agent_commands(self, task_description: str) -> list[TerminalCommand]:
        escaped_description = shlex.quote(task_description)
        
        return [
        # q chat with 30 min max timeout and also we wait on input. Using qchat because of sigv4. 
            TerminalCommand(
                command=f"qchat chat --no-interactive --trust-all-tools {escaped_description}",
                max_timeout_sec=1800, 
                block=True,
            )
        ]