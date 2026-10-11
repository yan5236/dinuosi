"""将已确认的品牌应用 PNG 导出为 Windows 多尺寸 ICO；需要 Pillow，不修改 SVG 或 PNG 原图。"""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SIZES = [(size, size) for size in (16, 20, 24, 32, 40, 48, 64, 128, 256)]


def prepare():
    """从现有 1024px 应用图标生成透明 ICO，并检查全部尺寸和最大帧像素。

    输出覆盖 assets/brand/dinuosi.ico；输入缺失、导出失败或透明通道丢失时抛出异常。
    """
    source = ROOT / "assets/brand/png/dinuosi-app-icon-1024.png"
    output = ROOT / "assets/brand/dinuosi.ico"
    with Image.open(source) as image:
        image = image.convert("RGBA")
        image.save(output, format="ICO", sizes=SIZES)
        with Image.open(output) as icon:
            assert icon.ico.sizes() == set(SIZES), "ICO 尺寸不完整"
            assert icon.ico.getimage((256, 256)).tobytes() == image.resize(
                (256, 256), Image.Resampling.LANCZOS
            ).tobytes(), "ICO 像素与品牌原图不同"
            assert icon.ico.getimage((32, 32)).getextrema()[3] == (0, 255), "透明通道丢失"
    print(f"已生成并验证品牌 ICO：{output}")


if __name__ == "__main__":
    prepare()
