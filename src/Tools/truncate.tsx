const truncateText = (text: string, maxWidth: number, font = "16px sans-serif") => {
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("2d");
    if (!context) return text;

    context.font = font;

    const ellipsis = "...";
    const ellipsisWidth = context.measureText(ellipsis).width;

    // If the full text fits, return it
    if (context.measureText(text).width <= maxWidth) return text;

    let truncated = text;
    while (truncated.length > 0) {
      truncated = truncated.slice(0, -1);
      if (context.measureText(truncated).width + ellipsisWidth <= maxWidth) {
        return truncated + ellipsis;
      }
    }

    return ellipsis; // Fallback in case nothing fits
  };