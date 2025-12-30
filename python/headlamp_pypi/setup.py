from __future__ import annotations

from wheel.bdist_wheel import bdist_wheel as _bdist_wheel
from setuptools import setup


class bdist_wheel(_bdist_wheel):
    """
    This distribution bundles a platform-specific binary, so wheels must be
    platform-tagged even though the Python sources are pure.
    """

    def finalize_options(self) -> None:
        super().finalize_options()
        setattr(self, "root_is_pure", False)

    def run(self) -> None:
        distribution = self.distribution
        original_has_ext_modules = distribution.has_ext_modules

        setattr(distribution, "has_ext_modules", lambda: True)
        try:
            super().run()
        finally:
            setattr(distribution, "has_ext_modules", original_has_ext_modules)

    def get_tag(self):
        _python_tag, _abi_tag, platform_tag = super().get_tag()
        # We ship one wheel per platform and it works across Python 3.
        return ("py3", "none", platform_tag)


setup(cmdclass={"bdist_wheel": bdist_wheel})
