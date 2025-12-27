package producer

import (
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"sync/atomic"
	"time"

	"rrproducer/pkg/utils"

	models "rrproducer/pkg/models/proto"

	"github.com/rabbitmq/rabbitmq-stream-go-client/pkg/amqp"
	stream "github.com/rabbitmq/rabbitmq-stream-go-client/pkg/stream"
	"go.uber.org/zap"
	"google.golang.org/protobuf/proto"
)

type StreamProducer struct {
	Env             *stream.Environment
	Producer        *stream.Producer
	StreamName      string
	Logger          *zap.Logger
	pendingConfirms int32 // Counter to track in-flight messages
}

func StartProducer() (*StreamProducer, error) {
	logger := utils.GetLogger()
	host := os.Getenv("RABBITMQ_ADVERTISED_HOST")
	portStr := os.Getenv("RABBITMQ_STREAM_PORT")
	port, err := strconv.Atoi(portStr)
	if err != nil {
		logger.Error("invalid port", zap.Error(err))
		return nil, err
	}

	username := os.Getenv("RABBITMQ_DEFAULT_USER")
	password := os.Getenv("RABBITMQ_DEFAULT_PASS")
	// Only connect to the existing stream
	streamName := os.Getenv("RABBITMQ_STREAM_NAME") + "_requests"

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

	producerOptions := stream.NewProducerOptions().
		SetProducerName("rrproducer")

	producer, err := env.NewProducer(streamName, producerOptions)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to stream '%s': %w", streamName, err)
	}

	app := &StreamProducer{
		Env:        env,
		Producer:   producer,
		StreamName: streamName,
		Logger:     logger,
	}

	chPublishConfirm := producer.NotifyPublishConfirmation()
	app.handlePublishConfirm(chPublishConfirm)

	return app, nil
}

func (sp *StreamProducer) handlePublishConfirm(confirms stream.ChannelPublishConfirm) {
	go func() {
		for confirmed := range confirms {
			for _, msg := range confirmed {
				if msg.IsConfirmed() {
					sp.Logger.Info("message confirmed and stored",
						zap.String("stream", sp.StreamName),
					)
				} else {
					sp.Logger.Error("message REJECTED by broker",
						zap.String("stream", sp.StreamName),
					)
				}
				// Decrease the counter of messages waiting for confirmation
				atomic.AddInt32(&sp.pendingConfirms, -1)
			}
		}
	}()
}

// WaitForConfirmations stays alive until all published messages are acknowledged
func (sp *StreamProducer) WaitForConfirmations(timeout time.Duration) {
	start := time.Now()
	for atomic.LoadInt32(&sp.pendingConfirms) > 0 {
		if time.Since(start) > timeout {
			sp.Logger.Warn("timed out waiting for broker confirmation")
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func (sp *StreamProducer) SendTaskJSON(task *models.TaskRequest) error {
	taskJSON, err := json.Marshal(task)
	if err != nil {
		return err
	}

	msg := amqp.NewMessage(taskJSON)
	msg.Properties = &amqp.MessageProperties{
		ContentType: "application/json",
	}

	atomic.AddInt32(&sp.pendingConfirms, 1)
	return sp.Producer.Send(msg)
}

func (sp *StreamProducer) SendTaskProtobuf(task *models.TaskRequest) error {
	taskProto, err := proto.Marshal(task)
	if err != nil {
		return err
	}

	msg := amqp.NewMessage(taskProto)
	msg.Properties = &amqp.MessageProperties{
		ContentType: "application/x-protobuf",
	}

	atomic.AddInt32(&sp.pendingConfirms, 1)
	return sp.Producer.Send(msg)
}
