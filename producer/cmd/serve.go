package cmd

import (
	"net"
	"os"

	producer "rrproducer/pkg/producer/service"
	app "rrproducer/pkg/server"
	"rrproducer/pkg/utils"

	pb "rrproducer/pkg/models/proto"

	"github.com/joho/godotenv"
	"github.com/spf13/cobra"
	"go.uber.org/zap"
	"google.golang.org/grpc"
	"google.golang.org/grpc/reflection"
)

var serveCmd = &cobra.Command{
	Use:   "serve",
	Short: "Start GRPC Server",
	Long:  "Starts a GRPC Server that listen to incoming TaskRequests",
	Run: func(cmd *cobra.Command, args []string) {
		serveTask()
	},
}

func init() {
	rootCmd.AddCommand(serveCmd)
}

func serveTask() {
	log := utils.GetLogger()
	if err := godotenv.Load("./.env"); err != nil {
		log.Fatal("No .env file found", zap.Error(err))
	}
	log.Info("Environment variables loaded successfully")
	streamProducer, err := producer.StartProducer()
	if err != nil {
		log.Fatal("Failed to create producer", zap.Error(err))
	}
	defer streamProducer.Producer.Close()
	grpcServer := grpc.NewServer()
	taskServer := app.NewServer(streamProducer)
	pb.RegisterTaskServiceServer(grpcServer, taskServer)

	// Register reflection service for grpcurl
	reflection.Register(grpcServer)

	port := os.Getenv("GRPC_PORT")
	if port == "" {
		port = "50051"
	}

	addr := ":" + port

	lis, err := net.Listen("tcp", addr)
	if err != nil {
		log.Fatal("Failed to listen",
			zap.String("addr", addr),
			zap.Error(err),
		)
	}

	log.Info("gRPC server started", zap.String("addr", addr))

	if err := grpcServer.Serve(lis); err != nil {
		log.Fatal("Failed to serve gRPC", zap.Error(err))
	}
}
