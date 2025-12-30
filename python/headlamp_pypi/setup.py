from __future__ import annotations

from wheel.bdist_wheel import bdist_wheel as _bdist_wheel


class bdist_wheel(_bdist_wheel):
    """
    This distribution bundles a platform-specific binary, so wheels must be
    platform-tagged even though the Python sources are pure.
    """

    def finalize_options(self) -> None:
        super().finalize_options()
        self.root_is_pure = False

    def get_tag(self):
        python, abi, plat = super().get_tag()
        # We ship one wheel per platform and it works across Python 3.
        return ("py3", "none", plat)


if __name__ == "__main__":
    from setuptools import setup

    setup(cmdclass={"bdist_wheel": bdist_wheel})
