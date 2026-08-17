package tsfacts_test

import (
	"bytes"
	"os"
	"path/filepath"
	"slices"
	"testing"

	"github.com/microsoft/typescript-go/internal/tsfacts"
	"gotest.tools/v3/assert"
)

func TestCanonicalJSONLinesFixturesRoundTrip(t *testing.T) {
	t.Parallel()
	fixtures, err := filepath.Glob("testdata/canonical/v0/*.jsonl")
	assert.NilError(t, err)
	assert.Equal(t, len(fixtures), 7)
	for _, fixture := range fixtures {
		t.Run(filepath.Base(fixture), func(t *testing.T) {
			t.Parallel()
			expected, readErr := os.ReadFile(fixture)
			assert.NilError(t, readErr)
			result, decodeErr := tsfacts.ReadJSONLines(bytes.NewReader(expected))
			assert.NilError(t, decodeErr)
			var actual bytes.Buffer
			assert.NilError(t, tsfacts.WriteJSONLines(&actual, result))
			assert.Equal(t, actual.String(), string(expected))
		})
	}
}

func TestReadJSONLinesRejectsUnknownRecordKind(t *testing.T) {
	t.Parallel()
	fixture, readErr := os.ReadFile("testdata/canonical/v0/occurrence-views.jsonl")
	assert.NilError(t, readErr)
	fixture = append(fixture, []byte(`{"record":"future"}`+"\n")...)
	_, err := tsfacts.ReadJSONLines(bytes.NewReader(fixture))
	assert.ErrorContains(t, err, `unknown record kind "future"`)
}

func TestReadJSONLinesAcceptsAdditiveFieldsAndCapabilities(t *testing.T) {
	t.Parallel()
	fixture, readErr := os.ReadFile("testdata/canonical/v0/occurrence-views.jsonl")
	assert.NilError(t, readErr)
	fixture = bytes.Replace(
		fixture,
		[]byte(`"protocol.fixtures.v0","types.advanced","types.core-composite"]`),
		[]byte(`"protocol.fixtures.v0","protocol.future","types.advanced","types.core-composite"]`),
		1,
	)
	fixture = bytes.Replace(
		fixture,
		[]byte(`"diagnosticCount":0}`),
		[]byte(`"diagnosticCount":0,"futureMetadata":{"enabled":true}}`),
		1,
	)

	result, err := tsfacts.ReadJSONLines(bytes.NewReader(fixture))
	assert.NilError(t, err)
	assert.Assert(t, slices.Contains(result.Header.Capabilities, "protocol.future"))
}
