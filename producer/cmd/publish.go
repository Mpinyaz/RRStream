package cmd

import (
	"encoding/json"
	"fmt"
	"os"
	"time"

	models "rrproducer/pkg/models/proto"
	producer "rrproducer/pkg/producer/service"
	"rrproducer/pkg/utils"

	"github.com/joho/godotenv"
	"github.com/spf13/cobra"
	"go.uber.org/zap"
)

var (
	jsonFile string
	envFile  string
)

var publishCmd = &cobra.Command{
	Use:   "publish",
	Short: "Publish a task to RabbitMQ stream",
	Long:  "Publishes a task from a JSON file to the RabbitMQ stream",
	Example: `  # Publish a single task from a JSON file
  rrproducer publish -f task.json

  # Publish using absolute path
  rrproducer publish --file /path/to/task.json`,
	Run: func(cmd *cobra.Command, args []string) {
		publishTask()
	},
}

func init() {
	publishCmd.Flags().StringVarP(&jsonFile, "file", "f", "", "Path to JSON file containing task (required)")
	publishCmd.Flags().StringVarP(&envFile, "env", "e", ".env", "Path to .env file")
	publishCmd.MarkFlagRequired("file")
}

func publishTask() {
	log := utils.GetLogger()
	defer utils.LogFlush()

	// Load environment variables
	if err := godotenv.Load(envFile); err != nil {
		log.Fatal("Failed to load .env file",
			zap.String("path", envFile),
			zap.Error(err),
		)
	}
	log.Info("Environment variables loaded", zap.String("path", envFile))

	// Read JSON file
	log.Info("Reading task file", zap.String("file", jsonFile))
	fileData, err := os.ReadFile(jsonFile)
	if err != nil {
		log.Fatal("Failed to read task file",
			zap.String("file", jsonFile),
			zap.Error(err),
		)
	}

	// Parse JSON into Task
	var taskData map[string]interface{}
	if err := json.Unmarshal(fileData, &taskData); err != nil {
		log.Fatal("Failed to parse JSON",
			zap.String("file", jsonFile),
			zap.Error(err),
		)
	}

	// Convert to protobuf Task
	task := &models.Task{
		Id:        getStringField(taskData, "id"),
		TaskType:  getStringField(taskData, "task_type"),
		Payload:   convertPayload(taskData["payload"]),
		CreatedAt: time.Now().Unix(),
	}

	// Set optional fields
	if priority, ok := taskData["priority"].(float64); ok {
		p := uint32(priority)
		task.Priority = &p
	}

	if retryCount, ok := taskData["retry_count"].(float64); ok {
		r := int32(retryCount)
		task.RetryCount = &r
	}

	// Override created_at if provided
	if createdAt, ok := taskData["created_at"].(float64); ok {
		task.CreatedAt = int64(createdAt)
	}

	log.Info("Parsed task",
		zap.String("id", task.Id),
		zap.String("type", task.TaskType),
	)

	// Start producer
	log.Info("Connecting to RabbitMQ stream")
	serve, err := producer.StartProducer()
	if err != nil {
		log.Fatal("Failed to create producer", zap.Error(err))
	}
	defer serve.Producer.Close()

	// Send task
	log.Info("Publishing task", zap.String("task_id", task.Id))
	if err := serve.SendTask(task); err != nil {
		log.Fatal("Failed to send task",
			zap.String("task_id", task.Id),
			zap.Error(err),
		)
	}

	// Wait a bit for confirmation
	time.Sleep(500 * time.Millisecond)

	log.Info("Task published successfully",
		zap.String("task_id", task.Id),
		zap.String("task_type", task.TaskType),
	)
	fmt.Printf("✓ Task '%s' published successfully\n", task.Id)
}

func getStringField(data map[string]interface{}, field string) string {
	if val, ok := data[field].(string); ok {
		return val
	}
	return ""
}

func convertPayload(payload interface{}) map[string]string {
	result := make(map[string]string)

	if payload == nil {
		return result
	}

	if payloadMap, ok := payload.(map[string]interface{}); ok {
		for key, value := range payloadMap {
			result[key] = fmt.Sprintf("%v", value)
		}
	}

	return result
}
