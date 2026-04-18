package service

import "testing"

func TestBuildDesktopRuntimeSyntheticAPIKey_IsStable(t *testing.T) {
	first := BuildDesktopRuntimeSyntheticAPIKey(7, 9, "platform-desktop")
	second := BuildDesktopRuntimeSyntheticAPIKey(7, 9, "platform-desktop")

	if first != second {
		t.Fatalf("expected deterministic key, got %q and %q", first, second)
	}
	if !IsDesktopRuntimeSyntheticAPIKey(first) {
		t.Fatalf("expected runtime key prefix, got %q", first)
	}
}

func TestBuildDesktopRuntimeSyntheticAPIKey_SeparatesProfiles(t *testing.T) {
	desktopKey := BuildDesktopRuntimeSyntheticAPIKey(7, 9, "platform-desktop")
	cliKey := BuildDesktopRuntimeSyntheticAPIKey(7, 9, "platform-cli")

	if desktopKey == cliKey {
		t.Fatalf("expected different runtime keys for different profiles")
	}
}
