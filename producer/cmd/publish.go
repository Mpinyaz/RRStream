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
	jsonFile    string
	envFile     string
	useProtobuf bool
)

var publishCmd = &cobra.Command{
	Use:   "publish",
	Short: "Publish a task to RabbitMQ stream",
	Long:  "Publishes a task from a JSON file to the RabbitMQ stream",
	Example: `  # Publish as JSON (default)
  rrproducer publish -f task.json

  # Publish as Protobuf
  rrproducer publish -f task.json --protobuf

  # Publish with custom .env location
  rrproducer publish -f task.json -e /path/to/.env --protobuf`,
	Run: func(cmd *cobra.Command, args []string) {
		publishTask()
	},
}

func init() {
	publishCmd.Flags().StringVarP(&jsonFile, "file", "f", "", "Path to JSON file containing task (required)")
	publishCmd.Flags().StringVarP(&envFile, "env", "e", "../.env", "Path to .env file")
	publishCmd.Flags().BoolVarP(&useProtobuf, "protobuf", "p", false, "Send as protobuf format (default: JSON)")
	publishCmd.MarkFlagRequired("file")
}

func publishTask() {
	log := utils.GetLogger()
	defer utils.LogFlush()

	// Load environment variables (optional if already set)
	if err := godotenv.Load(envFile); err != nil {
		log.Warn("Could not load .env file, using existing environment variables",
			zap.String("path", envFile),
		)
	} else {
		log.Info("Environment variables loaded", zap.String("path", envFile))
	}

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
	if err = json.Unmarshal(fileData, &taskData); err != nil {
		log.Fatal("Failed to parse JSON",
			zap.String("file", jsonFile),
			zap.Error(err),
		)
	}

	// Convert to protobuf Task
	task := &models.TaskRequest{
		Id:        getStringField(taskData, "id"),
		TaskType:  getStringField(taskData, "task_type"),
		Payload:   convertPayload(taskData["payload"]),
		CreatedAt: time.Now().Unix(),
	}

	// Validate required fields
	if task.Id == "" {
		log.Fatal("Task ID is required", zap.String("file", jsonFile))
	}
	if task.TaskType == "" {
		log.Fatal("Task type is required", zap.String("file", jsonFile))
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
		zap.Int("payload_fields", len(task.Payload)),
	)

	// Start producer
	log.Info("Connecting to RabbitMQ stream")
	serve, err := producer.StartProducer()
	if err != nil {
		log.Fatal("Failed to create producer", zap.Error(err))
	}
	defer serve.Producer.Close()

	// Send task based on format flag
	format := "JSON"
	if useProtobuf {
		format = "Protobuf"
	}

	log.Info("Publishing task",
		zap.String("task_id", task.Id),
		zap.String("format", format),
	)

	if useProtobuf {
		err = serve.SendTaskProtobuf(task)
	} else {
		err = serve.SendTaskJSON(task)
	}

	if err != nil {
		log.Fatal("Failed to send task",
			zap.String("task_id", task.Id),
			zap.String("format", format),
			zap.Error(err),
		)
	}

	// Wait a bit for confirmation
	time.Sleep(500 * time.Millisecond)

	log.Info("Task published successfully",
		zap.String("task_id", task.Id),
		zap.String("task_type", task.TaskType),
		zap.String("format", format),
	)

	// User-friendly output
	fmt.Printf("✓ Task '%s' published successfully as %s\n", task.Id, format)
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
