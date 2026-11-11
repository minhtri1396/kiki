//go:build darwin

package main

/*
#cgo darwin CFLAGS: -x objective-c
#cgo darwin LDFLAGS: -framework Cocoa
#import <Cocoa/Cocoa.h>

static void kikiPrepareWindow(void *window) {
	NSWindow *win = (__bridge NSWindow *)window;
	// A resizable window enters a fullscreen Space from the green button.
	// Without a menu bar, macOS never reveals the title bar there, so the
	// window cannot be zoomed back. The Window menu supplies that bar and
	// Control-Command-F.
	[win setCollectionBehavior:NSWindowCollectionBehaviorFullScreenPrimary | NSWindowCollectionBehaviorManaged];
	[win setContentMinSize:NSMakeSize(720, 480)];

	NSMenu *menubar = [[NSMenu alloc] init];

	NSMenuItem *appItem = [[NSMenuItem alloc] init];
	[menubar addItem:appItem];
	NSMenu *appMenu = [[NSMenu alloc] initWithTitle:@"KIKI"];
	NSMenuItem *quit = [[NSMenuItem alloc] initWithTitle:@"Quit KIKI" action:@selector(terminate:) keyEquivalent:@"q"];
	[appMenu addItem:quit];
	[appItem setSubmenu:appMenu];

	NSMenuItem *windowItem = [[NSMenuItem alloc] init];
	[menubar addItem:windowItem];
	NSMenu *windowMenu = [[NSMenu alloc] initWithTitle:@"Window"];
	NSMenuItem *minimize = [[NSMenuItem alloc] initWithTitle:@"Minimize" action:@selector(performMiniaturize:) keyEquivalent:@"m"];
	[minimize setTarget:win];
	NSMenuItem *zoom = [[NSMenuItem alloc] initWithTitle:@"Zoom" action:@selector(performZoom:) keyEquivalent:@""];
	[zoom setTarget:win];
	NSMenuItem *full = [[NSMenuItem alloc] initWithTitle:@"Toggle Full Screen" action:@selector(toggleFullScreen:) keyEquivalent:@"f"];
	[full setKeyEquivalentModifierMask:NSEventModifierFlagControl | NSEventModifierFlagCommand];
	[full setTarget:win];
	[windowMenu addItem:minimize];
	[windowMenu addItem:zoom];
	[windowMenu addItem:[NSMenuItem separatorItem]];
	[windowMenu addItem:full];
	[windowItem setSubmenu:windowMenu];

	[NSApp setMainMenu:menubar];
	[NSApp setWindowsMenu:windowMenu];
}
*/
import "C"
import "unsafe"

func prepareWindow(window unsafe.Pointer) {
	if window == nil {
		return
	}
	C.kikiPrepareWindow(window)
}
