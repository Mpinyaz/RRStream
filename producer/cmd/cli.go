package cmd

import "github.com/spf13/cobra"

var rootCmd = &cobra.Command{
	Use:   "RabbitMQ Stream scheduler",
	Short: "RRproducer Task Publisher CLI Application",
	Long:  "RabbitMQ is a simple CLI tool that allows you to send a task to the running RabbitMQ stream",
}

func Execute() {
	cobra.CheckErr(rootCmd.Execute())
}

func init() {
	rootCmd.AddCommand(startCmd)
	rootCmd.AddCommand(publishCmd)
}
