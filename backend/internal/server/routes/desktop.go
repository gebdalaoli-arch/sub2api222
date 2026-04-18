package routes

import (
	"github.com/Wei-Shaw/sub2api/internal/handler"
	servermiddleware "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

func RegisterDesktopRoutes(
	r *gin.Engine,
	v1 *gin.RouterGroup,
	h *handler.Handlers,
	jwtAuth servermiddleware.JWTAuthMiddleware,
	desktopAuth servermiddleware.DesktopRuntimeAuthMiddleware,
	settingService *service.SettingService,
) {
	if h.Desktop != nil {
		authenticated := v1.Group("/desktop")
		authenticated.Use(gin.HandlerFunc(jwtAuth))
		authenticated.Use(servermiddleware.BackendModeUserGuard(settingService))
		{
			authenticated.POST("/sessions", h.Desktop.CreateSession)
			authenticated.POST("/sessions/:id/refresh", h.Desktop.RefreshSession)
			authenticated.DELETE("/sessions/:id", h.Desktop.DeleteSession)
		}
	}

	if h.DesktopUpdates != nil {
		publicUpdates := v1.Group("/desktop/updates")
		{
			publicUpdates.GET("/check", h.DesktopUpdates.Check)
			publicUpdates.GET("/releases/:id", h.DesktopUpdates.GetRelease)
			publicUpdates.GET("/releases/:id/package", h.DesktopUpdates.DownloadPackage)
			publicUpdates.GET("/announcements", h.DesktopUpdates.ListAnnouncements)
		}
	}

	if h.OpenAIGateway != nil {
		desktopGateway := r.Group("/api/desktop/v1")
		desktopGateway.Use(gin.HandlerFunc(desktopAuth))
		{
			desktopGateway.POST("/responses", h.OpenAIGateway.Responses)
			desktopGateway.POST("/chat/completions", h.OpenAIGateway.ChatCompletions)
			desktopGateway.GET("/responses", h.OpenAIGateway.ResponsesWebSocket)
		}
	}
}
