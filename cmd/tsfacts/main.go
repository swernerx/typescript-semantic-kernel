package main

import (
	"context"
	"flag"
	"fmt"
	"io"
	"os"
	"os/signal"
	"syscall"

	"github.com/microsoft/typescript-go/internal/bundled"
	"github.com/microsoft/typescript-go/internal/json"
	"github.com/microsoft/typescript-go/internal/osutil"
	"github.com/microsoft/typescript-go/internal/tsfacts"
	"github.com/microsoft/typescript-go/internal/tspath"
	"github.com/microsoft/typescript-go/internal/vfs/osvfs"
)

func main() {
	os.Exit(runMain()) //nolint:forbidigo // The process entry point owns the exit status.
}

func runMain() int {
	defaultCurrentDirectory, err := os.Getwd() //nolint:forbidigo // The process entry point supplies the injectable working directory.
	if err != nil {
		fmt.Fprintf(os.Stderr, "tsfacts: get current directory: %v\n", err) //nolint:forbidigo // The process entry point owns stderr.
		return 1
	}
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM) //nolint:forbidigo // The process entry point owns signal handling.
	defer stop()
	return run(ctx, defaultCurrentDirectory, osutil.Args()[1:], os.Stdin, os.Stdout, os.Stderr) //nolint:forbidigo // The process entry point supplies injectable standard streams.
}

func run(ctx context.Context, defaultCurrentDirectory string, args []string, input io.Reader, output io.Writer, errorOutput io.Writer) int {
	flags := flag.NewFlagSet("tsfacts", flag.ContinueOnError)
	flags.SetOutput(errorOutput)
	currentDirectory := flags.String("cwd", defaultCurrentDirectory, "current directory used to resolve the project")
	if parseErr := flags.Parse(args); parseErr != nil {
		return 2
	}
	if flags.NArg() != 0 {
		fmt.Fprintln(errorOutput, "tsfacts: positional arguments are not supported; pass one request on standard input")
		return 2
	}

	var request tsfacts.Request
	if decodeErr := json.UnmarshalRead(input, &request); decodeErr != nil {
		fmt.Fprintf(errorOutput, "tsfacts: decode request: %v\n", decodeErr)
		return 2
	}
	fs := bundled.WrapFS(osvfs.FS())
	result, analyzeErr := tsfacts.Analyze(ctx, tsfacts.AnalyzerOptions{
		CurrentDirectory:   tspath.NormalizePath(*currentDirectory),
		FS:                 fs,
		DefaultLibraryPath: bundled.LibPath(),
	}, request)
	if analyzeErr != nil {
		fmt.Fprintf(errorOutput, "tsfacts: %v\n", analyzeErr)
		return 1
	}
	if writeErr := tsfacts.WriteJSONLines(output, result); writeErr != nil {
		fmt.Fprintf(errorOutput, "tsfacts: %v\n", writeErr)
		return 1
	}
	return 0
}
