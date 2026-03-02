from pathlib import Path

from fpdf import FPDF


ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "Autosample - CLI Autosampler User Guide.md"
DST = ROOT / "Autosample - CLI Autosampler User Guide.pdf"


def main() -> None:
    lines = SRC.read_text(encoding="utf-8").splitlines()

    pdf = FPDF()
    pdf.set_auto_page_break(auto=True, margin=12)
    pdf.add_page()

    in_code = False

    def write_line(text: str, line_height: float = 5.5) -> None:
        pdf.set_x(pdf.l_margin)
        pdf.multi_cell(pdf.epw, line_height, text)

    for raw in lines:
        line = raw.rstrip()

        if line.startswith("```"):
            in_code = not in_code
            pdf.ln(2)
            continue

        if line.startswith("# "):
            pdf.set_font("Helvetica", style="B", size=16)
            write_line(line[2:].strip(), 8)
            pdf.ln(1)
            continue
        if line.startswith("## "):
            pdf.set_font("Helvetica", style="B", size=13)
            write_line(line[3:].strip(), 7)
            pdf.ln(1)
            continue
        if line.startswith("### "):
            pdf.set_font("Helvetica", style="B", size=11)
            write_line(line[4:].strip(), 6)
            continue

        if in_code:
            pdf.set_font("Courier", size=9)
            text = line if line else " "
            write_line(text, 4.5)
            continue

        if line.startswith("|") and line.endswith("|"):
            # Keep markdown tables readable in plain text form.
            pdf.set_font("Helvetica", size=9)
            row = " | ".join(part.strip() for part in line.strip("|").split("|"))
            if row.replace("-", "").replace("|", "").strip() == "":
                continue
            write_line(row, 5)
            continue

        if line.startswith("- "):
            pdf.set_font("Helvetica", size=10)
            write_line(f"- {line[2:].strip()}", 5.5)
            continue

        pdf.set_font("Helvetica", size=10)
        write_line(line if line else " ", 5.5)

    pdf.output(str(DST))


if __name__ == "__main__":
    main()
