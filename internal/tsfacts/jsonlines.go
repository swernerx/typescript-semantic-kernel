package tsfacts

import (
	"errors"
	"fmt"
	"io"

	"github.com/microsoft/typescript-go/internal/json"
)

func WriteJSONLines(writer io.Writer, result *Result) error {
	if result == nil {
		return errors.New("result is required")
	}
	if err := writeJSONLine(writer, result.Header); err != nil {
		return err
	}
	for _, file := range result.Files {
		if err := writeJSONLine(writer, file); err != nil {
			return err
		}
	}
	for _, typ := range result.Types {
		if err := writeJSONLine(writer, typ); err != nil {
			return err
		}
	}
	for _, fact := range result.Facts {
		if err := writeJSONLine(writer, fact); err != nil {
			return err
		}
	}
	return nil
}

func writeJSONLine(writer io.Writer, value any) error {
	encoded, err := json.Marshal(value, json.Deterministic(true))
	if err != nil {
		return fmt.Errorf("encode JSON Lines record: %w", err)
	}
	encoded = append(encoded, '\n')
	written, err := writer.Write(encoded)
	if err != nil {
		return fmt.Errorf("write JSON Lines record: %w", err)
	}
	if written != len(encoded) {
		return fmt.Errorf("write JSON Lines record: %w", io.ErrShortWrite)
	}
	return nil
}
