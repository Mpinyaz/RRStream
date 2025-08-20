package producer

import (
	"fmt"
	"log"
	"os"
	"strconv"

	stream "github.com/rabbitmq/rabbitmq-stream-go-client/pkg/stream"
)

type StreamProducer struct {
	Env        *stream.Environment
	Producer   *stream.Producer
	StreamName string
}

func StartProducer() (*StreamProducer, error) {
	host := os.Getenv("RABBITMQ_ADVERTISED_HOST")
	portStr := os.Getenv("RABBITMQ_STREAM_PORT")
	port, err := strconv.Atoi(portStr)
	if err != nil {
		log.Printf("invalid port: %s", err.Error())
	}

	username := os.Getenv("RABBITMQ_DEFAULT_USER")
	password := os.Getenv("RABBITMQ_DEFAULT_PASS")
	streamName := os.Getenv("RABBITMQ_STREAM_NAME")

	env, err := stream.NewEnvironment(
		stream.NewEnvironmentOptions().
			SetHost(host).
			SetPort(port).
			SetUser(username).
			SetPassword(password),
	)
	if err != nil {
		return nil, fmt.Errorf("failed to create environment: %w", err)
	}

	err = env.DeclareStream(streamName, &stream.StreamOptions{MaxLengthBytes: stream.ByteCapacity{}.GB(5)})
	if err != nil {
		return nil, fmt.Errorf("failed to declare stream: %w", err)
	}

	producerOptions := stream.NewProducerOptions()
	producerOptions.SetProducerName("rrstreamproducer")
	producerOptions.SetCompression(stream.Compression{}.Gzip())

	producer, err := env.NewProducer(streamName, producerOptions)
	if err != nil {
		return nil, fmt.Errorf("failed to create producer: %w", err)
	}
	return &StreamProducer{
		Env:        env,
		Producer:   producer,
		StreamName: streamName,
	}, nil
}
