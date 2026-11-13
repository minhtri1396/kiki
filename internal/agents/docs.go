// Package agentdocs holds the KIKI agent instruction documents.
// go build embeds these markdown files in the console binary.
package agentdocs

import "embed"

//go:embed requirement development
var Files embed.FS
