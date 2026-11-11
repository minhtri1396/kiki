//go:build !darwin

package main

import "unsafe"

func prepareWindow(unsafe.Pointer) {}
