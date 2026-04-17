package handler

import (
	"context"
	"time"

	"github.com/Wei-Shaw/sub2api/internal/pkg/response"
	middleware2 "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

type desktopSessionService interface {
	Create(ctx context.Context, req service.DesktopSessionCreateRequest) (*service.DesktopSessionResult, error)
	Refresh(ctx context.Context, sessionID string) (*service.DesktopSessionResult, error)
	Revoke(ctx context.Context, sessionID string) error
}

type DesktopHandler struct {
	service desktopSessionService
}

type desktopSessionRequest struct {
	Target        string `json:"target" binding:"required,oneof=desktop cli"`
	DeviceID      string `json:"device_id" binding:"required"`
	DeviceName    string `json:"device_name" binding:"required"`
	ClientVersion string `json:"client_version" binding:"required"`
}

type desktopSessionResponse struct {
	SessionID      string        `json:"session_id"`
	UserID         int64         `json:"user_id"`
	RuntimeToken   string        `json:"runtime_token,omitempty"`
	ProfileKey     string        `json:"profile_key"`
	RefreshAfter   time.Duration `json:"refresh_after"`
	ExpiresAt      time.Time     `json:"expires_at"`
	GatewayBaseURL string        `json:"gateway_base_url"`
}

func NewDesktopHandler(service desktopSessionService) *DesktopHandler {
	return &DesktopHandler{service: service}
}

func (h *DesktopHandler) CreateSession(c *gin.Context) {
	subject, ok := middleware2.GetAuthSubjectFromContext(c)
	if !ok {
		response.Unauthorized(c, "User not authenticated")
		return
	}

	var req desktopSessionRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		response.BadRequest(c, "Invalid request: "+err.Error())
		return
	}

	result, err := h.service.Create(c.Request.Context(), service.DesktopSessionCreateRequest{
		UserID:        subject.UserID,
		DeviceID:      req.DeviceID,
		DeviceName:    req.DeviceName,
		Target:        service.DesktopSessionTarget(req.Target),
		ClientVersion: req.ClientVersion,
	})
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}

	response.Success(c, desktopSessionResultResponse(result, true))
}

func (h *DesktopHandler) RefreshSession(c *gin.Context) {
	result, err := h.service.Refresh(c.Request.Context(), c.Param("id"))
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}

	response.Success(c, desktopSessionResultResponse(result, false))
}

func (h *DesktopHandler) DeleteSession(c *gin.Context) {
	if err := h.service.Revoke(c.Request.Context(), c.Param("id")); err != nil {
		response.ErrorFrom(c, err)
		return
	}

	response.Success(c, gin.H{"message": "desktop session revoked"})
}

func desktopSessionResultResponse(result *service.DesktopSessionResult, includeRuntimeToken bool) desktopSessionResponse {
	if result == nil {
		return desktopSessionResponse{}
	}

	resp := desktopSessionResponse{
		SessionID:      result.SessionID,
		UserID:         result.UserID,
		ProfileKey:     result.ProfileKey,
		RefreshAfter:   result.RefreshAfter,
		ExpiresAt:      result.ExpiresAt,
		GatewayBaseURL: result.GatewayBaseURL,
	}
	if includeRuntimeToken {
		resp.RuntimeToken = result.RuntimeToken
	}
	return resp
}
