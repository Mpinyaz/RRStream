package main

import (
	"log"
	"rrproducer/producer"

	"github.com/joho/godotenv"
)

func main() {
	if err := godotenv.Load(); err != nil {
		log.Fatal("No .env file found!!!!")
	}

	serve, err := producer.StartProducer()
	if err != nil {
		log.Fatalf("Failed to create producer: %s", err.Error())
	}
	defer serve.Producer.Close()
}
