package docs

import (
	"fmt"
	"os/exec"

	"github.com/spf13/cobra"
)

func NewCmdDocs() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "docs",
		Short: "Get the settings documentation",
		Long:  "Get the settings documentation",
		Run: func(cmd *cobra.Command, arg []string) {
			fmt.Println("→ Opening Fig docs...")
			exec.Command("open", "https://fig.io/docs/support/settings").Run()
		},
	}

	return cmd
}
