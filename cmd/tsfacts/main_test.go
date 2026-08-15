package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"gotest.tools/v3/assert"
)

func TestRunWritesSemanticFacts(t *testing.T) {
	t.Parallel()
	projectDirectory := t.TempDir()
	assert.NilError(t, os.MkdirAll(filepath.Join(projectDirectory, "src"), 0o755))
	assert.NilError(t, os.WriteFile(
		filepath.Join(projectDirectory, "tsconfig.json"),
		[]byte(`{"compilerOptions":{"strict":true,"noEmit":true},"files":["src/example.ts"]}`),
		0o644,
	))
	const source = `const value = "hello" as const; value;`
	assert.NilError(t, os.WriteFile(filepath.Join(projectDirectory, "src", "example.ts"), []byte(source), 0o644))
	start := strings.LastIndex(source, "value")
	request := `{"schemaVersion":1,"project":"tsconfig.json","selections":[{"file":"src/example.ts","start":` +
		strconv.Itoa(start) + `,"end":` + strconv.Itoa(start+len("value")) + `}]}`

	var output bytes.Buffer
	var errorOutput bytes.Buffer
	status := run(t.Context(), projectDirectory, nil, strings.NewReader(request), &output, &errorOutput)

	assert.Equal(t, status, 0, errorOutput.String())
	assert.Equal(t, errorOutput.String(), "")
	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	assert.Assert(t, len(lines) >= 4)
	assert.Assert(t, strings.Contains(lines[0], `"record":"header"`))
	assert.Assert(t, strings.Contains(lines[len(lines)-1], `"record":"fact"`))
	assert.Assert(t, strings.Contains(lines[len(lines)-1], `"typeAtLocation":"type:`))
	assert.Assert(t, strings.Contains(lines[len(lines)-1], `"actualType":"type:`))
	assert.Assert(t, strings.Contains(lines[len(lines)-1], `"typeViewStates":{`))
	assert.Assert(t, strings.Contains(lines[len(lines)-1], `"inferredType":"type:`))
	assert.Assert(t, !strings.Contains(lines[len(lines)-1], `"annotationType"`))
	assert.Assert(t, !strings.Contains(lines[len(lines)-1], `"narrowedType"`))
	assert.Assert(t, strings.Contains(lines[len(lines)-1], `"symbol":"symbol:`))
}

func TestRunWritesFileWideSemanticFacts(t *testing.T) {
	t.Parallel()
	projectDirectory := t.TempDir()
	assert.NilError(t, os.MkdirAll(filepath.Join(projectDirectory, "src"), 0o755))
	assert.NilError(t, os.WriteFile(
		filepath.Join(projectDirectory, "tsconfig.json"),
		[]byte(`{"compilerOptions":{"strict":true,"noEmit":true},"files":["src/example.ts"]}`),
		0o644,
	))
	const source = `const value: string = "hello"; value;`
	assert.NilError(t, os.WriteFile(filepath.Join(projectDirectory, "src", "example.ts"), []byte(source), 0o644))
	request := `{"schemaVersion":1,"project":"tsconfig.json","files":["src/example.ts"]}`

	var output bytes.Buffer
	var errorOutput bytes.Buffer
	status := run(t.Context(), projectDirectory, nil, strings.NewReader(request), &output, &errorOutput)

	assert.Equal(t, status, 0, errorOutput.String())
	assert.Equal(t, errorOutput.String(), "")
	assert.Assert(t, strings.Count(output.String(), `"record":"fact"`) >= 4)
	assert.Assert(t, strings.Contains(output.String(), `"capabilities":["graph.references"`))
	assert.Assert(t, strings.Contains(output.String(), `"occurrence.file-wide"`))
}

func TestRunRejectsInvalidRequest(t *testing.T) {
	t.Parallel()
	var output bytes.Buffer
	var errorOutput bytes.Buffer
	status := run(t.Context(), "/project", nil, strings.NewReader(`{"schemaVersion":999}`), &output, &errorOutput)

	assert.Equal(t, status, 1)
	assert.Equal(t, output.String(), "")
	assert.Assert(t, strings.Contains(errorOutput.String(), "unsupported schemaVersion 999"))
}
