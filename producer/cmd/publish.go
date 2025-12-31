package cmd

import (
	"fmt"
	"os"
	"time"

	models "rrproducer/pkg/models/proto"
	producer "rrproducer/pkg/producer/service"
	"rrproducer/pkg/utils"

	"github.com/joho/godotenv"
	"github.com/spf13/cobra"
	"go.uber.org/zap"
	"google.golang.org/protobuf/encoding/protojson"
)

var (
	jsonFile    string
	envFile     string
	useProtobuf bool
)

var publishCmd = &cobra.Command{
	Use:   "publish",
	Short: "Publish a task to RabbitMQ stream",
	Long:  "Publishes a task from a JSON file to the RabbitMQ stream using a flat structure",
	Run: func(cmd *cobra.Command, args []string) {
		publishTask()
	},
}

func init() {
	publishCmd.Flags().StringVarP(&jsonFile, "file", "f", "", "Path to JSON file containing task (required)")
	publishCmd.Flags().StringVarP(&envFile, "env", "e", "./.env", "Path to .env file")
	publishCmd.Flags().BoolVarP(&useProtobuf, "protobuf", "p", false, "Send as protobuf format")
	publishCmd.MarkFlagRequired("file")
}

func publishTask() {
	log := utils.GetLogger()
	defer utils.LogFlush()

	if err := godotenv.Load(envFile); err != nil {
		log.Warn("Could not load .env file, using existing environment variables", zap.String("path", envFile))
	}

	// 1. Read JSON file
	fileData, err := os.ReadFile(jsonFile)
	if err != nil {
		log.Fatal("Failed to read task file", zap.Error(err))
	}

	// 2. Unmarshal directly into the flat TaskRequest struct
	// This automatically handles optional fields and nested UInt128 objects
	task := &models.TaskRequest{}
	unmarshaler := protojson.UnmarshalOptions{
		DiscardUnknown: true, // Ignores fields not in the proto
	}

	if err = unmarshaler.Unmarshal(fileData, task); err != nil {
		log.Fatal("Failed to parse JSON into flat TaskRequest", zap.Error(err))
	}

	// 3. Set server-side metadata if not provided in JSON
	if task.CreatedAt == 0 {
		task.CreatedAt = time.Now().Unix()
	}

	// 4. Validation
	if task.Id == "" || task.TaskType == models.TaskType_UNKNOWN {
		log.Fatal("Task 'id' and 'task_type' are required in JSON")
	}

	// 5. Connect and Publish
	serve, err := producer.StartProducer()
	if err != nil {
		log.Fatal("Failed to connect to producer", zap.Error(err))
	}

	log.Info("Publishing task", zap.String("id", task.Id), zap.Int("task type", int(task.TaskType)))

	if useProtobuf {
		err = serve.SendTaskProtobuf(task)
	} else {
		err = serve.SendTaskJSON(task)
	}

	if err != nil {
		log.Fatal("Failed to send task", zap.Error(err))
	}

	// 6. Wait for RabbitMQ acknowledgment
	serve.WaitForConfirmations(5 * time.Second)

	fmt.Printf("✓ Task '%s' confirmed and stored in stream\n", task.Id)

	serve.Producer.Close()
	serve.Env.Close()
}
