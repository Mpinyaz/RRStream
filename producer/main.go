package main

import (
	producer "rrproducer/producer/service"
	"rrproducer/utils"

	"github.com/joho/godotenv"
	"go.uber.org/zap"
)

func main() {
	// Initialize logger
	log := utils.GetLogger()
	defer utils.LogFlush()

	// Load environment variables
	if err := godotenv.Load("../.env"); err != nil {
		log.Fatal("No .env file found", zap.Error(err))
	}
	log.Info("Environment variables loaded successfully")

	// Start producer
	serve, err := producer.StartProducer()
	if err != nil {
		log.Fatal("Failed to create producer", zap.Error(err))
	}
	defer serve.Producer.Close()

	log.Info("Producer started successfully")
}
