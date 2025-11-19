package cmd

import (
	"rrproducer/pkg/producer"

	"github.com/spf13/cobra"
)

var startCmd = &cobra.Command{
	Use:   "start",
	Short: "Start up RabbitMQ stream",
	Long:  "Starts up the RabbitMQ stream if currently down",
	Run: func(cmd *cobra.Command, args []string) {
		producer.InitStream()
	},
}
