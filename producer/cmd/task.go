package cmd

import (
	"context"
	"fmt"
	"time"

	pb "rrproducer/pkg/models/proto"
	"rrproducer/pkg/utils"

	"github.com/joho/godotenv"
	"github.com/spf13/cobra"
	"go.uber.org/zap"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/protobuf/encoding/protojson"
)

var (
	grpcAddress string
	timeout     int
	payload     string
)

var taskCmd = &cobra.Command{
	Use:   "task",
	Short: "Submit a task to the gRPC server",
	Long:  "Submit a TaskRequest to the gRPC server using JSON payload",
	Example: `  # Create account
  rrproducer task --payload '{
    "id": "task-123",
    "taskType": 1,
    "createdAt": 1766864710,
    "ledger": 1,
    "code": 100,
    "accountId": {"low": 1000, "high": 0}
  }'

  # Create account batch
  rrproducer task --payload '{
    "id": "task-789",
    "taskType": 2,
    "createdAt": 1766864710,
    "accountBatch": [
      {"id": {"low": 1000, "high": 0}, "ledger": 1, "code": 100, "userData32": 1},
      {"id": {"low": 1001, "high": 0}, "ledger": 1, "code": 100, "userData32": 2}
    ]
  }'

  # Create transfer
  rrproducer task --payload '{
    "taskType": 4,
    "debitAccountId": {"low": 1000, "high": 0},
    "creditAccountId": {"low": 2000, "high": 0},
    "amount": {"low": 50000, "high": 0},
    "ledger": 1,
    "code": 200
  }'`,
	RunE: runTask,
}

func init() {
	rootCmd.AddCommand(taskCmd)

	taskCmd.Flags().StringVarP(&grpcAddress, "address", "a", "localhost:50051", "gRPC server address")
	taskCmd.Flags().IntVarP(&timeout, "timeout", "t", 10, "Request timeout in seconds")
	taskCmd.Flags().StringVarP(&payload, "payload", "p", "", "JSON payload for TaskRequest (required)")

	taskCmd.MarkFlagRequired("payload")
}

func runTask(cmd *cobra.Command, args []string) error {
	log := utils.GetLogger()
	if err := godotenv.Load("./.env"); err != nil {
		log.Fatal("No .env file found", zap.Error(err))
	}
	// Parse JSON payload into TaskRequest
	var taskReq pb.TaskRequest
	if err := protojson.Unmarshal([]byte(payload), &taskReq); err != nil {
		log.Fatal("failed to parse JSON payload", zap.Error(err))
	}

	// Generate task ID if not provided
	if taskReq.Id == "" {
		taskReq.Id = fmt.Sprintf("task-%d", time.Now().UnixNano())
	}

	// Set timestamp if not provided
	if taskReq.CreatedAt == 0 {
		taskReq.CreatedAt = time.Now().Unix()
	}

	// Connect to gRPC server
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(timeout)*time.Second)
	defer cancel()
	conn, err := grpc.NewClient(grpcAddress,
		grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return fmt.Errorf("failed to create gRPC client: %w", err)
	}
	defer conn.Close()

	client := pb.NewTaskServiceClient(conn)

	log.Info("📤 Submitting task...", zap.String("task_id:", taskReq.Id), zap.String("task_type:", taskReq.TaskType.String()))

	resp, err := client.SubmitTask(ctx, &taskReq)
	if err != nil {
		log.Fatal("failed to submit task: %w", zap.Error(err))
	}

	log.Info("✅ Task submitted successfully!")

	_ = resp
	return nil
}
