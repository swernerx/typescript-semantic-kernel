package api

import (
	"context"
	"errors"
	"testing"

	"github.com/microsoft/typescript-go/internal/bundled"
	"github.com/microsoft/typescript-go/internal/json"
	"github.com/microsoft/typescript-go/internal/semanticfacts"
	"github.com/microsoft/typescript-go/internal/testutil/projecttestutil"
	"gotest.tools/v3/assert"
)

func TestGetSemanticSnapshotUsesPinnedAPIProject(t *testing.T) {
	t.Parallel()
	if !bundled.Embedded {
		t.Skip("bundled files are not embedded")
	}

	const (
		configFileName = "/home/projects/p/tsconfig.json"
		fileName       = "/home/projects/p/src/index.ts"
		logicalFile    = "src/index.ts"
		baseText       = "export const value = 1;"
		temporaryText  = `export const value = "changed";`
	)
	files := map[string]any{
		configFileName: `{ "compilerOptions": { "strict": true } }`,
		fileName:       baseText,
	}
	projectSession, _ := projecttestutil.Setup(files)
	defer projectSession.Close()
	session := NewSession(projectSession, nil)
	defer session.Close()

	ctx := context.Background()
	base, err := session.handleUpdateSnapshot(ctx, &UpdateSnapshotParams{
		OpenFiles: []DocumentIdentifier{{FileName: fileName}},
	})
	assert.NilError(t, err)
	assert.Equal(t, len(base.Projects), 1)
	projectID := base.Projects[0].Id
	selection := semanticfacts.Selection{File: logicalFile, Start: 13, End: 18}
	request := func(snapshot SnapshotID) *GetSemanticSnapshotParams {
		return &GetSemanticSnapshotParams{
			Snapshot:      snapshot,
			Project:       projectID,
			SchemaVersion: semanticfacts.SchemaVersion,
			Files:         []string{logicalFile},
			Selections:    []semanticfacts.Selection{selection},
		}
	}

	first, err := session.handleGetSemanticSnapshot(ctx, request(base.Snapshot))
	assert.NilError(t, err)
	second, err := session.handleGetSemanticSnapshot(ctx, request(base.Snapshot))
	assert.NilError(t, err)
	firstJSON, err := json.Marshal(first)
	assert.NilError(t, err)
	secondJSON, err := json.Marshal(second)
	assert.NilError(t, err)
	assert.DeepEqual(t, firstJSON, secondJSON)
	assert.Equal(t, first.Header.SchemaVersion, semanticfacts.SchemaVersion)
	assert.Equal(t, first.Header.OffsetEncoding, semanticfacts.OffsetEncoding)
	assert.Equal(t, len(first.Facts), 1)
	baseDisplay := displayForFact(t, first, first.Facts[0])

	temporary, err := session.handleUpdateTemporarySnapshot(ctx, &UpdateTemporarySnapshotParams{
		Snapshot: base.Snapshot,
		File:     DocumentIdentifier{FileName: fileName},
		NewText:  temporaryText,
	})
	assert.NilError(t, err)
	defer func() {
		_, releaseErr := session.handleRelease(context.Background(), &ReleaseParams{Snapshot: temporary.Snapshot})
		assert.NilError(t, releaseErr)
	}()

	temporaryFacts, err := session.handleGetSemanticSnapshot(ctx, request(temporary.Snapshot))
	assert.NilError(t, err)
	temporaryDisplay := displayForFact(t, temporaryFacts, temporaryFacts.Facts[0])
	assert.Assert(t, temporaryDisplay != baseDisplay, "temporary snapshot must expose its overlaid semantic type")

	baseAgain, err := session.handleGetSemanticSnapshot(ctx, request(base.Snapshot))
	assert.NilError(t, err)
	assert.Equal(t, displayForFact(t, baseAgain, baseAgain.Facts[0]), baseDisplay)

	invalid := request(base.Snapshot)
	invalid.Files = []string{"src/missing.ts"}
	invalid.Selections = nil
	_, err = session.handleGetSemanticSnapshot(ctx, invalid)
	assert.ErrorContains(t, err, "source file \"src/missing.ts\" is not part of project")

	cancelled, cancel := context.WithCancel(ctx)
	cancel()
	_, err = session.handleGetSemanticSnapshot(cancelled, request(base.Snapshot))
	assert.Assert(t, errors.Is(err, context.Canceled))
}

func displayForFact(t *testing.T, result *semanticfacts.Result, fact semanticfacts.FactRecord) string {
	t.Helper()
	for _, record := range result.Types {
		if record.ID == fact.ActualType {
			return record.Display
		}
	}
	t.Fatalf("actual type %q not found", fact.ActualType)
	return ""
}
