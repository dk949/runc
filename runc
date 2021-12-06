#!/usr/bin/python3.9
from enum import IntEnum, auto
from typing import Callable, Optional, TypedDict

import argparse
import os
import subprocess as sp
import sys
import tempfile


def _runPython(file: str, _: list[str]) -> sp.CompletedProcess[bytes]:
    return sp.run(["python", file], stdout=sp.PIPE, stderr=sp.PIPE)


def _runC(file: str, usedFiles: list[str]) -> sp.CompletedProcess[bytes]:
    outfile = "/tmp/a.out"
    usedFiles.append(outfile)
    r = sp.run(
        ["gcc", file, "-O3", "-o", outfile], stdout=sp.PIPE, stderr=sp.PIPE)
    if r.returncode != 0:
        return r
    return sp.run("/tmp/a.out", stdout=sp.PIPE, stderr=sp.PIPE)


class ExitCode(IntEnum):
    INTERNAL_ERROR = -1
    OK = auto()
    LANGUAGE_ERROR = auto()
    EDITOR_ERROR = auto()
    FILE_ERROR = auto()
    CODE_ERROR = auto()


class RunException(Exception):
    def __init__(self, errorCode: ExitCode, msg: str) -> None:
        super().__init__(errorCode, msg)
        self.errorCode = errorCode
        self.msg = msg


class Runner:

    class LangT(TypedDict):
        runner: Callable[[str, list[str]], sp.CompletedProcess]
        extension: str

    LangsT = dict[str, LangT]

    _lang: str
    _langs: LangsT = {
        "python": {"runner": _runPython, "extension": ".py"},
        "c": {"runner": _runC, "extension": ".c"}
    }
    _editor: str
    _file: str
    _usedFiles: list[str] = []
    ret: ExitCode

    def _makeFile(self) -> str:
        with open(os.path.join(tempfile.gettempdir(), f"runc_runner{self._langs[self._lang]['extension']}"), "w") as f:
            self._usedFiles.append(f.name)
            return f.name

    def _getLang(self, lang: Optional[str]) -> str:
        if not lang:
            raise RunException(ExitCode.LANGUAGE_ERROR,
                               "Language has to be specified")
        if lang not in self._langs:
            raise RunException(ExitCode.LANGUAGE_ERROR,
                               "Unsupported language")
        return lang

    def _getEditor(self) -> str:
        if "EDITOR" not in os.environ:
            raise RunException(ExitCode.EDITOR_ERROR,
                               "Could not determine editor. Try setting EDITOR environment variable.")
        return os.environ["EDITOR"]

    def _openEditor(self) -> str:
        f = self._makeFile()
        r = sp.run([self._editor, f])
        if r.returncode != 0:
            raise RunException(ExitCode.EDITOR_ERROR,
                               f"Failed to run the editor. Command {r.args} failed with {r.returncode}:\n\t{r.stderr.decode('utf8')}")
        return f

    def __init__(self, lang: Optional[str]) -> None:
        try:
            self.ret = ExitCode.OK
            self._lang = self._getLang(lang)
            self._editor = self._getEditor()
            self._file = self._openEditor()
            self.run()
        except RunException as re:
            print(re.msg, file=sys.stderr)
            self.ret = re.errorCode

    def __del__(self) -> None:
        for file in self._usedFiles:
            os.remove(file)

    def run(self) -> None:
        r = self._langs[self._lang]["runner"](self._file, self._usedFiles)
        if r.returncode != 0:
            raise RunException(ExitCode.CODE_ERROR,
                               f"Command {r.args} failed with {r.returncode}:\n\t{r.stderr.decode('utf8')}")

        print(r.stdout.decode('utf8'))


def main(lang: Optional[str]) -> int:
    return Runner(lang).ret


def parseArgs() -> Optional[str]:
    parser = argparse.ArgumentParser(description='run code.')
    parser.add_argument('lang', metavar='LANG', type=str,
                        nargs='?', help='language to be ran')
    # maybe more args later idk
    return parser.parse_args().lang


if __name__ == "__main__":
    try:
        exit(main(parseArgs()))
    except Exception as e:
        print("Unexpected exception occurred:", e)
        exit(ExitCode.INTERNAL_ERROR)
