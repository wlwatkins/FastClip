import { ActionIcon, Button, Grid, Tooltip } from "@mantine/core";
import { IconTag, IconEdit, IconTrashX } from "@tabler/icons-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import FastClip from "../Classes/FastClip";
import { useDisclosure } from "@mantine/hooks";
import Edit from "./EditClip";
import Delete from "./DeleteClip";
import { useState, useEffect, useRef } from "react";
import { useViewportSize } from "@mantine/hooks";

interface ItemProps {
  fast_clip: FastClip;
}

const Clip: React.FC<ItemProps> = ({ fast_clip }) => {
  const { width } = useViewportSize();
  const [editOpened, { open: openEdit, close: closeEdit }] = useDisclosure(false);
  const [deleteOpened, { open: openDelete, close: closeDelete }] = useDisclosure(false);
  const [_copied, setCopied] = useState(false);
  const [_truncatedLabel, setTruncatedLabel] = useState(fast_clip.label);
  const buttonRef = useRef<HTMLButtonElement>(null);

  // Function to measure text width and truncate accordingly
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

  // Use ResizeObserver to detect button size changes
  useEffect(() => {
    const buttonElement = buttonRef.current;
    if (!buttonElement) return;

    const observer = new ResizeObserver(() => {
      const maxWidth = buttonElement.clientWidth - 30; // Subtract padding & icons
      setTruncatedLabel(truncateText(fast_clip.label, maxWidth));
    });

    observer.observe(buttonElement);

    return () => observer.disconnect();
  }, [fast_clip.label]);

  const handlePutInClipBoard = () => {
    writeText(fast_clip.value)
      .then(() => {
        console.log("Text written successfully");
      })
      .catch((error) => {
        console.error("Error writing text:", error);
      });
  };

  const handleCopy = () => {
    handlePutInClipBoard();
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <>
      <Grid w="100%"  justify="center" align="center">
        <Grid.Col span="auto">
            <Tooltip withArrow multiline label={
              <div style={{
                whiteSpace: 'nowrap',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                width: '100%'
              }}>
                {fast_clip.value}
              </div>
            } position="bottom" color="gray" w={width * 0.90}>



              {/* <motion.div
                  style={{
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                    flex: 1,
                  }}
                  exit={{ opacity: 1 }}
                  animate={{ opacity: copied ? 0 : 1 }}
                  transition={{ duration: 1 }}
                  key={copied ? 'copied' : fast_clip.label}
                > */}

              {/* </motion.div> */}




              <Button
                ref={buttonRef} // Reference for measuring width
                fullWidth
                autoContrast
                justify="space-between"
                leftSection={<IconTag size={14} />}
                variant="filled"
                color={fast_clip.colour}
                radius="md"
                size="xs"
                onClick={handleCopy}
                
              >
                  {fast_clip.label}
              </Button>


            </Tooltip>
        </Grid.Col>

        <Grid.Col span="content">
          <ActionIcon.Group>
            <ActionIcon variant="light" size="sm" onClick={openEdit}>
              <IconEdit size={15} stroke={1.5} />
            </ActionIcon>

            <ActionIcon variant="light" color="red" size="sm" onClick={openDelete}>
              <IconTrashX size={15} stroke={1.5} />
            </ActionIcon>
          </ActionIcon.Group>
        </Grid.Col>
      </Grid>

      <Edit fast_clip={fast_clip} open={openEdit} close={closeEdit} opened={editOpened} />
      <Delete fast_clip={fast_clip} open={openDelete} close={closeDelete} opened={deleteOpened} />
    </>
  );
};

export default Clip;
