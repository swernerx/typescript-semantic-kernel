package api

import (
	"context"
	"io"
	"testing"

	"github.com/microsoft/typescript-go/internal/json"
	"github.com/microsoft/typescript-go/internal/jsonrpc"
	"gotest.tools/v3/assert"
)

func TestAsyncConnCancelsJSONRPCRequest(t *testing.T) {
	t.Parallel()
	started := make(chan struct{})
	protocol := &cancellationTestProtocol{errors: make(chan *jsonrpc.ResponseError, 1)}
	handler := cancellationTestHandler{handle: func(ctx context.Context) (any, error) {
		close(started)
		<-ctx.Done()
		return nil, ctx.Err()
	}}
	conn := NewAsyncConnWithProtocol(nopReadWriteCloser{}, protocol, handler)
	id := jsonrpc.NewIDInt(42)
	conn.startRequest(context.Background(), &Message{ID: id, Method: "slow"})
	<-started

	params, err := json.Marshal(struct {
		ID int32 `json:"id"`
	}{ID: 42})
	assert.NilError(t, err)
	conn.cancelRequest(params)

	responseError := <-protocol.errors
	assert.Equal(t, responseError.Code, int32(-32800))
	assert.Equal(t, responseError.Message, context.Canceled.Error())
}

type cancellationTestHandler struct {
	handle func(context.Context) (any, error)
}

func (h cancellationTestHandler) HandleRequest(ctx context.Context, _ string, _ json.Value) (any, error) {
	return h.handle(ctx)
}

func (cancellationTestHandler) HandleNotification(context.Context, string, json.Value) error {
	return nil
}

type cancellationTestProtocol struct {
	errors chan *jsonrpc.ResponseError
}

func (*cancellationTestProtocol) ReadMessage() (*Message, error) {
	return nil, io.EOF
}

func (*cancellationTestProtocol) WriteRequest(*jsonrpc.ID, string, any) error {
	return nil
}

func (*cancellationTestProtocol) WriteNotification(string, any) error {
	return nil
}

func (*cancellationTestProtocol) WriteResponse(*jsonrpc.ID, any) error {
	return nil
}

func (p *cancellationTestProtocol) WriteError(_ *jsonrpc.ID, err *jsonrpc.ResponseError) error {
	p.errors <- err
	return nil
}

type nopReadWriteCloser struct{}

func (nopReadWriteCloser) Read([]byte) (int, error) {
	return 0, io.EOF
}

func (nopReadWriteCloser) Write(data []byte) (int, error) {
	return len(data), nil
}

func (nopReadWriteCloser) Close() error {
	return nil
}
