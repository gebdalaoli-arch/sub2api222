#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEPLOY_DIR="${REPO_ROOT}/deploy"
COMPOSE_FILE="${DEPLOY_DIR}/docker-compose.local.yml"
ENV_FILE="${DEPLOY_DIR}/.env"
BACKUP_ROOT="${DEPLOY_DIR}/backups"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP_DIR="${BACKUP_ROOT}/incremental-update-${TIMESTAMP}"

print_info() {
  printf '[INFO] %s\n' "$1"
}

print_error() {
  printf '[ERROR] %s\n' "$1" >&2
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    print_error "缺少命令: $1"
    exit 1
  fi
}

copy_if_exists() {
  local src="$1"
  local dst_dir="$2"
  if [ -f "$src" ]; then
    cp "$src" "$dst_dir/"
  fi
}

healthcheck_url() {
  local port="8080"
  if [ -f "$ENV_FILE" ]; then
    local env_port
    env_port="$(grep -E '^SERVER_PORT=' "$ENV_FILE" | tail -n1 | cut -d'=' -f2- || true)"
    if [ -n "${env_port}" ]; then
      port="${env_port}"
    fi
  fi
  printf 'http://127.0.0.1:%s/health' "$port"
}

main() {
  require_command docker
  require_command curl

  if ! docker compose version >/dev/null 2>&1; then
    print_error '当前环境不可用 docker compose'
    exit 1
  fi

  if [ ! -f "${COMPOSE_FILE}" ]; then
    print_error "未找到部署文件: ${COMPOSE_FILE}"
    exit 1
  fi

  if [ ! -f "${ENV_FILE}" ]; then
    print_error "未找到环境文件: ${ENV_FILE}"
    exit 1
  fi

  if [ ! -f "${REPO_ROOT}/Dockerfile" ]; then
    print_error "未找到仓库根目录 Dockerfile: ${REPO_ROOT}/Dockerfile"
    exit 1
  fi

  mkdir -p "${BACKUP_DIR}"
  print_info "备份部署文件到 ${BACKUP_DIR}"
  copy_if_exists "${ENV_FILE}" "${BACKUP_DIR}"
  copy_if_exists "${COMPOSE_FILE}" "${BACKUP_DIR}"
  copy_if_exists "${DEPLOY_DIR}/config.yaml" "${BACKUP_DIR}"
  copy_if_exists "${DEPLOY_DIR}/data/config.yaml" "${BACKUP_DIR}"

  print_info '校验 docker compose 配置'
  (
    cd "${DEPLOY_DIR}"
    docker compose -f docker-compose.local.yml config >/dev/null
  )

  print_info '使用当前仓库源码构建本地镜像 weishaw/sub2api:latest'
  (
    cd "${REPO_ROOT}"
    docker build -f Dockerfile -t weishaw/sub2api:latest .
  )

  print_info '重启本地目录版 docker compose 部署'
  (
    cd "${DEPLOY_DIR}"
    docker compose -f docker-compose.local.yml up -d
  )

  local url
  url="$(healthcheck_url)"
  print_info "执行健康检查: ${url}"
  curl -fsS "${url}" >/dev/null

  cat <<EOF

[DONE] 增量更新已执行完成。

建议继续执行以下人工验证：

1. 查看后端日志
   cd "${DEPLOY_DIR}"
   docker compose -f docker-compose.local.yml logs --tail=200 sub2api

2. 查看健康接口
   curl -fsS "${url}"

3. 客户端侧验证
   - 登录是否正常
   - 平台 CLI 是否能真实问答
   - 平台 Desktop 是否能创建会话
   - 扣费后 /api/v1/usage 是否出现记录

4. 如需回滚，优先恢复以下备份：
   ${BACKUP_DIR}

EOF
}

main "$@"
