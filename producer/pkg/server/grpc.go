package server

import (
	"context"
	"fmt"

	pb "rrproducer/pkg/models/proto"
	producer "rrproducer/pkg/producer/service"

	"go.uber.org/zap"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
	"google.golang.org/protobuf/encoding/protojson"
)

type Server struct {
	Producer *producer.StreamProducer
	pb.UnimplementedTaskServiceServer
}

func NewServer(producer *producer.StreamProducer) *Server {
	return &Server{
		Producer: producer,
	}
}

func (s *Server) SubmitTask(ctx context.Context, in *pb.TaskRequest) (*pb.Empty, error) {
	msg := fmt.Sprintf("Submitted %s request", in.TaskType.String())
	s.Producer.Logger.Info(msg,
		zap.String("task id", in.Id))

	if err := s.Producer.SendTaskProtobuf(in); err != nil {
		s.Producer.Logger.Error("Failed to publish task", zap.Error(err))
		return nil, status.Errorf(codes.Internal, "failed to publish task: %v", err)
	}

	s.Producer.Logger.Info("Task published successfully", zap.String("task_id", in.Id))

	return &pb.Empty{}, nil
}

func (s *Server) PublishResponse(ctx context.Context, in *pb.TaskResponse) (*pb.Empty, error) {
	msg := fmt.Sprintf("Received %s response", in.TaskType)
	s.Producer.Logger.Info(msg,
		zap.String("task id", in.Id))
	jsonBytes, err := protojson.MarshalOptions{
		Multiline:       false,
		EmitUnpopulated: true,
		UseProtoNames:   true,
	}.Marshal(in)

	if err != nil {
		s.Producer.Logger.Error(
			"failed to marshal TaskResponse",
			zap.Error(err),
			zap.String("task_id", in.Id),
		)
	} else {
		s.Producer.Logger.Info(
			"📥 received task response",
			zap.String("task_id", in.Id),
			zap.String("task_type", in.TaskType),
			zap.String("payload", string(jsonBytes)),
		)
	}
	return &pb.Empty{}, nil
}
