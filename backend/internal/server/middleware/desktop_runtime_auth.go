package middleware

import (
	"context"
	"strings"

	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

const contextKeyDesktopSession = "desktop_session"

type desktopRuntimeTokenValidator interface {
	ValidateRuntimeToken(ctx context.Context, token string) (*service.DesktopSession, error)
}

type DesktopRuntimeAuthMiddleware gin.HandlerFunc

func NewDesktopRuntimeAuthMiddleware(svc desktopRuntimeTokenValidator) DesktopRuntimeAuthMiddleware {
	return DesktopRuntimeAuthMiddleware(func(c *gin.Context) {
		authHeader := strings.TrimSpace(c.GetHeader("Authorization"))
		if authHeader == "" {
			AbortWithError(c, 401, "RUNTIME_TOKEN_REQUIRED", "Runtime token is required")
			return
		}

		parts := strings.SplitN(authHeader, " ", 2)
		if len(parts) != 2 || !strings.EqualFold(parts[0], "Bearer") {
			AbortWithError(c, 401, "INVALID_RUNTIME_TOKEN", "Invalid runtime token")
			return
		}

		token := strings.TrimSpace(parts[1])
		if token == "" {
			AbortWithError(c, 401, "INVALID_RUNTIME_TOKEN", "Invalid runtime token")
			return
		}

		session, err := svc.ValidateRuntimeToken(c.Request.Context(), token)
		if err != nil {
			AbortWithError(c, 401, "INVALID_RUNTIME_TOKEN", "Invalid runtime token")
			return
		}

		c.Set(contextKeyDesktopSession, session)
		c.Set(string(ContextKeyUser), AuthSubject{UserID: session.UserID})
		c.Next()
	})
}
