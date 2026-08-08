"""Infraestrutura local-first V2 do ARCZ Earth.

A importação deste pacote não abre sockets nem cria dados fictícios. O servidor
existente instancia :class:`V2Router` explicitamente.
"""
from .errors import ApiError
from .network_policy import NetworkMode, NetworkPolicy, install_egress_guard
from .project_migrations import CURRENT_PROJECT_SCHEMA, migrate_project, migrate_project_file
from .v2_router import V2Router

__all__ = ["ApiError", "NetworkMode", "NetworkPolicy", "install_egress_guard", "CURRENT_PROJECT_SCHEMA", "migrate_project", "migrate_project_file", "V2Router"]
