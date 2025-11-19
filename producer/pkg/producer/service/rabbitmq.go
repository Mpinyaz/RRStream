package producer

import (
	"encoding/json"
	"fmt"
	"os"
	"strconv"

	models "rrproducer/pkg/models/proto"
	"rrproducer/pkg/utils"

	"github.com/google/uuid"
	"github.com/rabbitmq/rabbitmq-stream-go-client/pkg/amqp"
	stream "github.com/rabbitmq/rabbitmq-stream-go-client/pkg/stream"
	"go.uber.org/zap"
)

type StreamProducer struct {
	Env        *stream.Environment
	Producer   *stream.Producer
	StreamName string
	Logger     *zap.Logger
}

func StartProducer() (*StreamProducer, error) {
	logger := utils.GetLogger()
	host := os.Getenv("RABBITMQ_ADVERTISED_HOST")
	portStr := os.Getenv("RABBITMQ_STREAM_PORT")
	port, err := strconv.Atoi(portStr)
	if err != nil {
		logger.Panic("invalid port:", zap.Error(err))
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
		logger.Error("failed to create environment", zap.Error(err))
		return nil, fmt.Errorf("failed to create environment: %w", err)
	}

	err = env.DeclareStream(streamName, &stream.StreamOptions{MaxLengthBytes: stream.ByteCapacity{}.GB(5)})
	if err != nil {
		logger.Error("failed to declare stream", zap.Error(err))
		return nil, fmt.Errorf("failed to declare stream: %w", err)
	}
	producerOptions := stream.NewProducerOptions().
		SetProducerName(streamName)

	producer, err := env.NewProducer(streamName, producerOptions)
	if err != nil {
		logger.Error("failed to create producer", zap.Error(err))
		return nil, fmt.Errorf("failed to create producer: %w", err)
	}

	app := StreamProducer{
		Env:        env,
		Producer:   producer,
		StreamName: streamName,
		Logger:     utils.GetLogger(),
	}

	chPublishConfirm := producer.NotifyPublishConfirmation()
	app.handlePublishConfirm(chPublishConfirm)

	return &app, nil
}

func (sp *StreamProducer) handlePublishConfirm(confirms stream.ChannelPublishConfirm) {
	go func() {
		for confirmed := range confirms {
			for _, msg := range confirmed {
				if msg.IsConfirmed() {
					infoMsg := fmt.Sprintf("message confirmed -> %s", msg.GetMessage().GetData())
					sp.Logger.Info(infoMsg,
						zap.String("status", "stored"),
					)
				} else {
					errMsg := fmt.Sprintf("message confirmation failed -> %s", msg.GetMessage().GetData())
					sp.Logger.Error(errMsg,
						zap.String("status", "failed"),
					)
				}
			}
		}
	}()
}

func (sp *StreamProducer) SendTask(task *models.Task) error {
	taskJSON, err := json.Marshal(task)
	if err != nil {
		sp.Logger.Error("failed to marshal task", zap.Error(err))
		return fmt.Errorf("failed to marshal task: %w", err)
	}
	msg := amqp.NewMessage(taskJSON)
	props := amqp.MessageProperties{
		CorrelationID: uuid.NewString(),
	}
	msg.Properties = &props

	if err = sp.Producer.Send(msg); err != nil {
		sp.Logger.Error("failed to send task", zap.Error(err))
		return fmt.Errorf("failed to send task: %w", err)
	}
	return nil
}

// func (sp *StreamProducer) handleJSONPublish() error {
// }
