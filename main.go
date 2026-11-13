package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"time"

	"github.com/webview/webview_go"

	"kiki/internal/gitignore"
	"kiki/internal/jobs"
	"kiki/internal/orca"
	"kiki/internal/server"
	"kiki/internal/store"
)

func main() {
	addr := flag.String("addr", "127.0.0.1:8787", "listen address")
	noOpen := flag.Bool("no-open", false, "serve without opening the window")
	flag.Parse()

	root, err := appDir()
	if err != nil {
		log.Fatal(err)
	}
	if err := store.Migrate(root); err != nil {
		log.Fatal(err)
	}
	setupPath := store.Path(root)
	db, err := store.Open(setupPath)
	if err != nil {
		log.Fatal(err)
	}
	defer db.Close()

	if err := db.EnsureLayout(root); err != nil {
		log.Fatal(err)
	}
	agentPath, err := server.EnsureAgentFiles(root, db)
	if err != nil {
		log.Fatal(err)
	}
	ignorePath, err := gitignore.Ensure(root, gitignore.AppEntries(root, agentPath, setupPath, db.LogFile()))
	if err != nil {
		log.Fatal(err)
	}

	app := &server.Server{
		Root:  root,
		Store: db,
		Orca:  &orca.Client{Dir: root},
		Jobs:  jobs.New(),
	}
	httpServer := &http.Server{
		Addr:              *addr,
		Handler:           app.Handler(),
		ReadHeaderTimeout: 10 * time.Second,
	}
	url := "http://" + *addr + "/"
	fmt.Printf("KIKI: %s\nRepository: %s\nSetup: %s\nAgents: %s\nLogs: %s\n", url, root, setupPath, agentPath, db.LogFile())
	if ignorePath != "" {
		fmt.Printf("Git ignore: %s\n", ignorePath)
	}
	if *noOpen {
		if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatal(err)
		}
		return
	}

	errCh := make(chan error, 1)
	go func() {
		if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			errCh <- err
		}
	}()
	if err := waitReady(url, errCh); err != nil {
		log.Fatal(err)
	}
	runWindow(url)
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	_ = httpServer.Shutdown(ctx)
}

func waitReady(url string, errCh <-chan error) error {
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		select {
		case err := <-errCh:
			return err
		default:
		}
		resp, err := http.Get(url)
		if err == nil {
			resp.Body.Close()
			return nil
		}
		time.Sleep(30 * time.Millisecond)
	}
	return fmt.Errorf("KIKI did not start at %s", url)
}

func runWindow(url string) {
	window := webview.New(false)
	defer window.Destroy()
	window.SetTitle("KIKI")
	window.SetSize(1100, 760, webview.HintNone)
	prepareWindow(window.Window())
	window.Navigate(url)
	window.Run()
}

func appDir() (string, error) {
	exe, err := os.Executable()
	if err != nil {
		return "", err
	}
	resolved, err := filepath.EvalSymlinks(exe)
	if err != nil {
		return "", err
	}
	return filepath.Abs(filepath.Dir(resolved))
}
