package service

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"strings"
)

const DesktopRuntimeSyntheticAPIKeyPrefix = "desktop-runtime-"

// BuildDesktopRuntimeSyntheticAPIKey returns a deterministic hidden API key used
// for desktop runtime billing and usage attribution. The key is stable per
// user/group/profile so usage logs can reference a persistent api_key_id
// without creating a new row for every desktop session.
func BuildDesktopRuntimeSyntheticAPIKey(userID, groupID int64, profileKey string) string {
	normalizedProfileKey := strings.TrimSpace(profileKey)
	if normalizedProfileKey == "" {
		normalizedProfileKey = "desktop-runtime"
	}

	sum := sha256.Sum256([]byte(fmt.Sprintf("%d:%d:%s", userID, groupID, normalizedProfileKey)))
	return DesktopRuntimeSyntheticAPIKeyPrefix + hex.EncodeToString(sum[:16])
}

func IsDesktopRuntimeSyntheticAPIKey(key string) bool {
	return strings.HasPrefix(strings.TrimSpace(key), DesktopRuntimeSyntheticAPIKeyPrefix)
}
