package cmd

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"rrproducer/pkg/utils"

	pb "rrproducer/pkg/models/proto"

	"github.com/gin-gonic/gin"
	"github.com/joho/godotenv"
	"github.com/spf13/cobra"
	"go.uber.org/zap"
	"google.golang.org/grpc"
	"google.golang.org/grpc/connectivity"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/protobuf/encoding/protojson"
)

var proxyCmd = &cobra.Command{
	Use:   "proxy",
	Short: "Starts a HTTP API Gateway to the GRPC backend",
	Long:  "Starts a HTTP API Gateway that handles TaskRequests and TaskResponses",
	Run: func(cmd *cobra.Command, args []string) {
		runProxy()
	},
}

func init() {
	rootCmd.AddCommand(proxyCmd)
}

func NewTaskServiceClient(addr string) (pb.TaskServiceClient, *grpc.ClientConn, error) {
	conn, err := grpc.NewClient(
		addr,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		return nil, nil, fmt.Errorf("failed to create gRPC client: %w", err)
	}

	client := pb.NewTaskServiceClient(conn)
	return client, conn, nil
}

func runProxy() {
	log := utils.GetLogger()

	// Load environment variables
	if err := godotenv.Load("./.env"); err != nil {
		log.Warn("No .env file found, using environment defaults", zap.Error(err))
	}
	log.Info("Environment variables loaded successfully")

	gatewayAddr := os.Getenv("GATEWAY_ADDR")
	if gatewayAddr == "" {
		gatewayAddr = ":8080"
		log.Info("GATEWAY_ADDR not set, using default", zap.String("addr", gatewayAddr))
	}

	grpcAddr := os.Getenv("GRPC_ADDR")
	if grpcAddr == "" {
		grpcAddr = "localhost:50051"
		log.Info("GRPC_ADDR not set, using default", zap.String("addr", grpcAddr))
	}

	// Create gRPC client (lazy connection)
	log.Info("Creating gRPC client", zap.String("addr", grpcAddr))
	client, conn, err := NewTaskServiceClient(grpcAddr)
	if err != nil {
		log.Fatal("Could not create grpc client",
			zap.String("addr", grpcAddr),
			zap.Error(err))
	}
	defer conn.Close()

	log.Info("gRPC client created successfully", zap.String("addr", grpcAddr))

	// Setup Gin router
	if os.Getenv("GIN_MODE") == "" {
		gin.SetMode(gin.ReleaseMode)
	}
	r := gin.Default()

	// Request logging middleware
	r.Use(func(c *gin.Context) {
		start := time.Now()
		path := c.Request.URL.Path

		c.Next()

		log.Info("HTTP Request",
			zap.String("method", c.Request.Method),
			zap.String("path", path),
			zap.Int("status", c.Writer.Status()),
			zap.Duration("duration", time.Since(start)),
			zap.String("client_ip", c.ClientIP()),
		)
	})

	// Configure protojson unmarshaler
	unmarshaler := protojson.UnmarshalOptions{
		AllowPartial:   true,
		DiscardUnknown: false, // Reject unknown fields for safety
	}

	// Submit task endpoint
	r.POST("/task/submit", func(c *gin.Context) {
		// Read raw body
		body, err := io.ReadAll(c.Request.Body)
		if err != nil {
			log.Error("Failed to read request body", zap.Error(err))
			c.JSON(http.StatusBadRequest, gin.H{"error": "Failed to read request body"})
			return
		}

		log.Debug("Received JSON request", zap.String("body", string(body)))

		// Unmarshal using protojson
		var req pb.TaskRequest
		if err := unmarshaler.Unmarshal(body, &req); err != nil {
			log.Error("Failed to unmarshal JSON to protobuf",
				zap.Error(err),
				zap.String("body", string(body)))
			c.JSON(http.StatusBadRequest, gin.H{
				"error":   "Invalid JSON format for protobuf",
				"details": err.Error(),
			})
			return
		}

		log.Info("Task request parsed successfully",
			zap.String("task_id", req.Id),
			zap.String("task_type", req.TaskType.String()))

		// Call gRPC service
		ctx, cancel := context.WithTimeout(c.Request.Context(), 10*time.Second)
		defer cancel()

		resp, err := client.SubmitTask(ctx, &req)
		if err != nil {
			log.Error("Failed to submit task",
				zap.Error(err),
				zap.String("task_id", req.Id))
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}

		log.Info("Task submitted successfully", zap.String("task_id", req.Id))
		c.JSON(http.StatusOK, resp)
	})

	// Task response endpoint
	r.POST("/task/response", func(c *gin.Context) {
		// Read raw body
		body, err := io.ReadAll(c.Request.Body)
		if err != nil {
			log.Error("Failed to read request body", zap.Error(err))
			c.JSON(http.StatusBadRequest, gin.H{"error": "Failed to read request body"})
			return
		}

		log.Debug("Received JSON response", zap.String("body", string(body)))

		// Unmarshal using protojson
		var req pb.TaskResponse
		if err := unmarshaler.Unmarshal(body, &req); err != nil {
			log.Error("Failed to unmarshal JSON to protobuf",
				zap.Error(err),
				zap.String("body", string(body)))
			c.JSON(http.StatusBadRequest, gin.H{
				"error":   "Invalid JSON format for protobuf",
				"details": err.Error(),
			})
			return
		}

		// Call gRPC service
		ctx, cancel := context.WithTimeout(c.Request.Context(), 10*time.Second)
		defer cancel()

		resp, err := client.PublishResponse(ctx, &req)
		if err != nil {
			log.Error("Failed to publish response",
				zap.Error(err),
				zap.String("task_id", req.Id))
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}

		log.Info("Response published successfully", zap.String("task_id", req.Id))
		c.JSON(http.StatusOK, resp)
	})

	// Health check endpoint with actual gRPC connection test
	r.GET("/health", func(c *gin.Context) {
		state := conn.GetState()

		// Try to connect if idle
		if state == connectivity.Idle {
			conn.Connect()
		}

		// Check if ready or idle (both are acceptable)
		healthy := state == connectivity.Ready || state == connectivity.Idle

		status := http.StatusOK
		if !healthy {
			status = http.StatusServiceUnavailable
		}

		c.JSON(status, gin.H{
			"status":     map[bool]string{true: "healthy", false: "unhealthy"}[healthy],
			"grpc":       grpcAddr,
			"grpc_state": state.String(),
			"timestamp":  time.Now().Unix(),
		})
	})

	// Readiness check endpoint
	r.GET("/ready", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"status":    "ready",
			"timestamp": time.Now().Unix(),
		})
	})

	// Start server with graceful shutdown
	srv := &http.Server{
		Addr:         gatewayAddr,
		Handler:      r,
		ReadTimeout:  15 * time.Second,
		WriteTimeout: 15 * time.Second,
		IdleTimeout:  60 * time.Second,
	}

	// Graceful shutdown
	go func() {
		log.Info("Starting HTTP gateway", zap.String("addr", gatewayAddr))
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatal("Failed to start server", zap.Error(err))
		}
	}()

	// Wait for interrupt signal
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	log.Info("Shutting down server...")

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := srv.Shutdown(ctx); err != nil {
		log.Error("Server forced to shutdown", zap.Error(err))
	}

	log.Info("Server exited gracefully")
}
