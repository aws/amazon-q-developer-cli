package community

import (
	"fmt"
	"os/exec"

	"github.com/spf13/cobra"
)

func NewCmdCommunity() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "community",
		Short: "Join the Fig community",
		Run: func(cmd *cobra.Command, arg []string) {
			fmt.Printf("\n→ Joining Fig community....\n\n")
			exec.Command("open", "https://fig.io/community").Run()
		},
	}

	return cmd
}
