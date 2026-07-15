#!/usr/bin/env python3
"""Shared, dependency-free planning and interactive menu helpers for RRiter builds."""

from __future__ import annotations

import os
import sys
from io import StringIO
from dataclasses import dataclass
from enum import Enum
from typing import Callable, Iterable, Sequence, TextIO


class PlanError(ValueError):
    """Raised when a requested build plan contains incompatible choices."""


class PgoMode(str, Enum):
    OFF = "off"
    FRESH = "fresh"
    REUSE = "reuse"


@dataclass(frozen=True)
class BuildPlan:
    """Platform-neutral work requested from a Windows or macOS build script."""

    run_tests: bool
    build: bool
    release: bool = True
    package: bool = True
    installer: bool = False
    run_after_build: bool = False
    pgo: PgoMode = PgoMode.OFF
    pgo_profile: str | None = None

    def validate(self) -> "BuildPlan":
        if not self.run_tests and not self.build:
            raise PlanError("the plan must run tests, build RRiter, or both")
        if not self.build:
            if self.package or self.installer or self.run_after_build:
                raise PlanError("tests-only mode cannot package, install, or run a binary")
            if self.pgo is not PgoMode.OFF:
                raise PlanError("tests-only mode cannot use PGO")
        if not self.release and self.pgo is not PgoMode.OFF:
            raise PlanError("PGO is supported only for release builds")
        if self.installer and not self.package:
            raise PlanError("an installer requires packaging to be enabled")
        if self.pgo is PgoMode.REUSE and self.pgo_profile == "":
            raise PlanError("the PGO profile path cannot be empty")
        if self.pgo is PgoMode.OFF and self.pgo_profile is not None:
            raise PlanError("--pgo-profile requires --pgo reuse or --pgo fresh")
        return self

    @property
    def profile_name(self) -> str:
        return "release" if self.release else "debug"

    @property
    def action_name(self) -> str:
        if self.run_tests and self.build:
            return "tests + build"
        if self.run_tests:
            return "tests only"
        return "build only"

    def phases(self) -> tuple[str, ...]:
        phases: list[str] = []
        if self.run_tests:
            phases.append("tests")
        if self.build:
            if self.pgo is PgoMode.FRESH:
                phases.extend(("instrumented build", "automated GUI training", "profile merge"))
            elif self.pgo is PgoMode.REUSE:
                phases.append("profile validation")
            phases.append("PGO build" if self.pgo is not PgoMode.OFF else "build")
        if self.package:
            phases.append("package")
        if self.installer:
            phases.append("installer")
        if self.run_after_build:
            phases.append("run")
        return tuple(phases)

    def summary_lines(self) -> tuple[str, ...]:
        return (
            f"Action:       {self.action_name}",
            f"Profile:      {self.profile_name if self.build else 'n/a'}",
            f"PGO:          {self.pgo.value if self.build else 'n/a'}",
            f"Package:      {'yes' if self.package else 'no'}",
            f"Installer:    {'yes' if self.installer else 'no'}",
            f"Run after:    {'yes' if self.run_after_build else 'no'}",
            f"Phases:       {' -> '.join(self.phases())}",
        )


@dataclass(frozen=True)
class MenuChoice:
    label: str
    value: object
    detail: str = ""


def is_interactive(
    stdin: TextIO | None = None,
    stdout: TextIO | None = None,
) -> bool:
    source = sys.stdin if stdin is None else stdin
    destination = sys.stdout if stdout is None else stdout
    return bool(getattr(source, "isatty", lambda: False)()) and bool(
        getattr(destination, "isatty", lambda: False)()
    )


def should_open_menu(
    argv: Sequence[str],
    *,
    force: bool = False,
    stdin: TextIO | None = None,
    stdout: TextIO | None = None,
) -> bool:
    return force or (not argv and is_interactive(stdin, stdout))


def _supports_color(stream: TextIO) -> bool:
    return (
        os.environ.get("NO_COLOR") is None
        and bool(getattr(stream, "isatty", lambda: False)())
        and os.environ.get("TERM", "") != "dumb"
    )


def _style(text: str, code: str, stream: TextIO) -> str:
    if not _supports_color(stream):
        return text
    return f"\x1b[{code}m{text}\x1b[0m"


def print_header(
    title: str,
    subtitle: str,
    *,
    output: TextIO | None = None,
) -> None:
    stream = sys.stdout if output is None else output
    width = max(56, len(title) + 8, len(subtitle) + 8)
    border = "=" * width
    print(_style(border, "36", stream), file=stream)
    print(_style(f"  {title}", "1;36", stream), file=stream)
    print(f"  {subtitle}", file=stream)
    print(_style(border, "36", stream), file=stream)


def choose(
    prompt: str,
    choices: Sequence[MenuChoice],
    *,
    default: int = 0,
    input_fn: Callable[[str], str] = input,
    output: TextIO | None = None,
) -> object:
    if not choices:
        raise PlanError("menu choice list cannot be empty")
    if default < 0 or default >= len(choices):
        raise PlanError("menu default index is out of range")
    stream = sys.stdout if output is None else output
    print(file=stream)
    print(_style(prompt, "1", stream), file=stream)
    for index, choice in enumerate(choices, start=1):
        marker = "*" if index - 1 == default else " "
        detail = f" - {choice.detail}" if choice.detail else ""
        print(f"  {marker} {index}. {choice.label}{detail}", file=stream)
    while True:
        answer = input_fn(f"Select [default {default + 1}]: ").strip()
        if not answer:
            return choices[default].value
        try:
            selected = int(answer) - 1
        except ValueError:
            print("Enter the number of an option.", file=stream)
            continue
        if 0 <= selected < len(choices):
            return choices[selected].value
        print(f"Choose a number from 1 to {len(choices)}.", file=stream)


def confirm(
    prompt: str,
    *,
    default: bool,
    input_fn: Callable[[str], str] = input,
    output: TextIO | None = None,
) -> bool:
    stream = sys.stdout if output is None else output
    suffix = "[Y/n]" if default else "[y/N]"
    while True:
        answer = input_fn(f"{prompt} {suffix}: ").strip().lower()
        if not answer:
            return default
        if answer in {"y", "yes", "1", "true"}:
            return True
        if answer in {"n", "no", "0", "false"}:
            return False
        print("Enter y or n.", file=stream)


def interactive_build_plan(
    platform_name: str,
    *,
    supports_installer: bool,
    input_fn: Callable[[str], str] = input,
    output: TextIO | None = None,
) -> BuildPlan:
    stream = sys.stdout if output is None else output
    print_header(
        "RRiter build menu",
        f"Tests, builds, packaging, and PGO for {platform_name}",
        output=stream,
    )
    action = choose(
        "What should be done?",
        (
            MenuChoice("Tests only", "tests", "do not build or package"),
            MenuChoice("Build only", "build"),
            MenuChoice("Tests, then build", "tests-build"),
        ),
        default=2,
        input_fn=input_fn,
        output=stream,
    )
    build = action != "tests"
    run_tests = action != "build"
    if not build:
        return BuildPlan(
            run_tests=True,
            build=False,
            package=False,
            installer=False,
            run_after_build=False,
        ).validate()

    release = bool(
        choose(
            "Build profile",
            (
                MenuChoice("Release", True, "optimized build"),
                MenuChoice("Debug", False, "faster compile, no PGO"),
            ),
            default=0,
            input_fn=input_fn,
            output=stream,
        )
    )
    pgo = PgoMode.OFF
    if release:
        pgo = PgoMode(
            choose(
                "Profile-guided optimization",
                (
                    MenuChoice("No PGO", PgoMode.OFF.value),
                    MenuChoice(
                        "Create a fresh profile automatically",
                        PgoMode.FRESH.value,
                        "opens RRiter, exercises the UI, closes it, then rebuilds",
                    ),
                    MenuChoice(
                        "Reuse a compatible profile",
                        PgoMode.REUSE.value,
                        "validates the saved profile before building",
                    ),
                ),
                default=0,
                input_fn=input_fn,
                output=stream,
            )
        )
    package = confirm(
        f"Create the standard {platform_name} package?",
        default=True,
        input_fn=input_fn,
        output=stream,
    )
    installer = False
    if supports_installer and package:
        installer = confirm(
            "Also create the installer?",
            default=False,
            input_fn=input_fn,
            output=stream,
        )
    run_after = confirm(
        "Run RRiter after a successful build?",
        default=False,
        input_fn=input_fn,
        output=stream,
    )
    return BuildPlan(
        run_tests=run_tests,
        build=True,
        release=release,
        package=package,
        installer=installer,
        run_after_build=run_after,
        pgo=pgo,
    ).validate()


def print_plan(
    plan: BuildPlan,
    *,
    platform_lines: Iterable[str] = (),
    output: TextIO | None = None,
) -> None:
    stream = sys.stdout if output is None else output
    print(file=stream)
    print(_style("Selected plan", "1;36", stream), file=stream)
    for line in (*plan.summary_lines(), *tuple(platform_lines)):
        print(f"  {line}", file=stream)


def default_build_plan(
    *,
    run_tests: bool,
    tests_only: bool,
    debug: bool,
    package: bool,
    installer: bool,
    run_after_build: bool,
    pgo: str,
    pgo_profile: str | None,
) -> BuildPlan:
    build = not tests_only
    return BuildPlan(
        run_tests=run_tests or tests_only,
        build=build,
        release=not debug,
        package=package if build else False,
        installer=installer if build else False,
        run_after_build=run_after_build if build else False,
        pgo=PgoMode(pgo) if build else PgoMode.OFF,
        pgo_profile=pgo_profile,
    ).validate()


def self_test() -> None:
    release = BuildPlan(True, True, pgo=PgoMode.FRESH).validate()
    if release.phases()[:4] != (
        "tests",
        "instrumented build",
        "automated GUI training",
        "profile merge",
    ):
        raise PlanError("fresh-PGO phase order self-test failed")
    tests = BuildPlan(True, False, package=False).validate()
    if tests.action_name != "tests only":
        raise PlanError("tests-only plan self-test failed")
    try:
        BuildPlan(False, True, release=False, pgo=PgoMode.FRESH).validate()
    except PlanError:
        pass
    else:
        raise PlanError("debug PGO must be rejected")

    class TtyBuffer(StringIO):
        def isatty(self) -> bool:
            return True

    terminal = TtyBuffer()
    if not should_open_menu([], stdin=terminal, stdout=terminal):
        raise PlanError("no-argument interactive menu self-test failed")
    if should_open_menu([], stdin=StringIO(), stdout=StringIO()):
        raise PlanError("non-interactive menu guard self-test failed")

    answers = iter(("3", "1", "2", "n", "n"))
    menu_plan = interactive_build_plan(
        "self-test",
        supports_installer=False,
        input_fn=lambda _prompt: next(answers),
        output=StringIO(),
    )
    if not menu_plan.run_tests or menu_plan.pgo is not PgoMode.FRESH:
        raise PlanError("interactive plan selection self-test failed")
    if menu_plan.package or menu_plan.run_after_build:
        raise PlanError("interactive plan confirmation self-test failed")

    try:
        BuildPlan(False, True, pgo_profile="profile.profdata").validate()
    except PlanError:
        pass
    else:
        raise PlanError("a profile path without PGO must be rejected")


__all__ = [
    "BuildPlan",
    "MenuChoice",
    "PgoMode",
    "PlanError",
    "choose",
    "confirm",
    "default_build_plan",
    "interactive_build_plan",
    "is_interactive",
    "print_header",
    "print_plan",
    "self_test",
    "should_open_menu",
]
