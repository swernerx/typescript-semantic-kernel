package tsfacts

import (
	"bufio"
	"bytes"
	"errors"
	"fmt"
	"io"

	"github.com/microsoft/typescript-go/internal/json"
)

const maxJSONLineBytes = 16 << 20

// ReadJSONLines decodes and validates one complete semantic graph response.
// It exists primarily as an executable consumer oracle for canonical fixtures.
func ReadJSONLines(reader io.Reader) (*Result, error) {
	scanner := bufio.NewScanner(reader)
	scanner.Buffer(make([]byte, 64<<10), maxJSONLineBytes)
	result := &Result{}
	headerSeen := false
	lineNumber := 0
	lastPhase := 0
	for scanner.Scan() {
		lineNumber++
		line := bytes.TrimSpace(scanner.Bytes())
		if len(line) == 0 {
			return nil, fmt.Errorf("decode JSON Lines record %d: empty records are not allowed", lineNumber)
		}
		var envelope struct {
			Record string `json:"record"`
		}
		if err := json.Unmarshal(line, &envelope); err != nil {
			return nil, fmt.Errorf("decode JSON Lines record %d: %w", lineNumber, err)
		}
		if lineNumber == 1 && envelope.Record != "header" {
			return nil, errors.New("decode JSON Lines record 1: header must be first")
		}
		phase, known := jsonLineRecordPhase(envelope.Record)
		if !known {
			return nil, fmt.Errorf("decode JSON Lines record %d: unknown record kind %q", lineNumber, envelope.Record)
		}
		if phase < lastPhase {
			return nil, fmt.Errorf("decode JSON Lines record %d: %q record is out of canonical order", lineNumber, envelope.Record)
		}
		lastPhase = phase
		switch envelope.Record {
		case "header":
			if headerSeen {
				return nil, fmt.Errorf("decode JSON Lines record %d: duplicate header", lineNumber)
			}
			headerSeen = true
			if err := json.Unmarshal(line, &result.Header); err != nil {
				return nil, fmt.Errorf("decode JSON Lines header: %w", err)
			}
		case "file":
			if err := appendJSONRecord(line, &result.Files); err != nil {
				return nil, fmt.Errorf("decode JSON Lines file record %d: %w", lineNumber, err)
			}
		case "type":
			if err := appendJSONRecord(line, &result.Types); err != nil {
				return nil, fmt.Errorf("decode JSON Lines type record %d: %w", lineNumber, err)
			}
		case "declaration":
			if err := appendJSONRecord(line, &result.Declarations); err != nil {
				return nil, fmt.Errorf("decode JSON Lines declaration record %d: %w", lineNumber, err)
			}
		case "symbol":
			if err := appendJSONRecord(line, &result.Symbols); err != nil {
				return nil, fmt.Errorf("decode JSON Lines symbol record %d: %w", lineNumber, err)
			}
		case "signature":
			if err := appendJSONRecord(line, &result.Signatures); err != nil {
				return nil, fmt.Errorf("decode JSON Lines signature record %d: %w", lineNumber, err)
			}
		case "fact":
			if err := appendJSONRecord(line, &result.Facts); err != nil {
				return nil, fmt.Errorf("decode JSON Lines fact record %d: %w", lineNumber, err)
			}
		}
	}
	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("read JSON Lines response: %w", err)
	}
	if !headerSeen {
		return nil, errors.New("decode JSON Lines response: header is required")
	}
	if err := ValidateResult(result); err != nil {
		return nil, fmt.Errorf("validate semantic graph: %w", err)
	}
	return result, nil
}

func jsonLineRecordPhase(record string) (int, bool) {
	switch record {
	case "header":
		return 0, true
	case "file":
		return 1, true
	case "type":
		return 2, true
	case "declaration":
		return 3, true
	case "symbol":
		return 4, true
	case "signature":
		return 5, true
	case "fact":
		return 6, true
	default:
		return 0, false
	}
}

func appendJSONRecord[Record any](line []byte, records *[]Record) error {
	var record Record
	if err := json.Unmarshal(line, &record); err != nil {
		return err
	}
	*records = append(*records, record)
	return nil
}
