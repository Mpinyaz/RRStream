package utils

type Task struct {
	ID         string                 `json:"id"`
	TaskType   string                 `json:"task_type"`
	Payload    map[string]interface{} `json:"payload"`
	CreatedAt  int64                  `json:"created_at"`
	Priority   *uint8                 `json:"priority,omitempty"`
	RetryCount *int                   `json:"retry_count,omitempty"`
}
