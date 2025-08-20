package producer

import (
	"log"
	"os"
	"strconv"

	stream "github.com/rabbitmq/rabbitmq-stream-go-client/pkg/stream"
)

func StartProducer() {
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

	err = env.DeclareStream(streamName, &stream.StreamOptions{MaxLengthBytes: stream.ByteCapacity{}.GB(5)})
}
