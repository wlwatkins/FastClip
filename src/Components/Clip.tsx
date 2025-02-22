import { ActionIcon, Button, Grid, Tooltip } from "@mantine/core";
import { IconTag, IconEdit, IconTrashX } from "@tabler/icons-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import FastClip from "../Classes/FastClip";
import { useDisclosure } from "@mantine/hooks";
import Edit from "./EditClip";
import Delete from "./DeleteClip";
import { useState, useEffect, useRef } from "react";
import { useViewportSize } from "@mantine/hooks";
import { invoke } from "@tauri-apps/api/core";
import { notifications } from "@mantine/notifications";

interface ItemProps {
  fast_clip: FastClip;
}


const Clip: React.FC<ItemProps> = ({ fast_clip }) => {
  const fastClipRef = useRef<FastClip>(fast_clip);
  const { width } = useViewportSize();
  const [editOpened, { open: openEdit, close: closeEdit }] = useDisclosure(false);
  const [deleteOpened, { open: openDelete, close: closeDelete }] = useDisclosure(false);
  const [_copied, setCopied] = useState(false);
  const [_truncatedLabel, setTruncatedLabel] = useState(fastClipRef.current.label);
  const buttonRef = useRef<HTMLButtonElement>(null);



  useEffect(() => {
    const buttonElement = buttonRef.current;
    if (!buttonElement) return;

    const observer = new ResizeObserver(() => {
      setTruncatedLabel(fastClipRef.current.label);
    });

    observer.observe(buttonElement);

    return () => observer.disconnect();
  }, [fastClipRef.current.label]);

  const handlePutInClipBoard = () => {

    notifications.show({
      title: `Copied '${fastClipRef.current.label}' to clipboard`,
      message: undefined,
      withCloseButton: false,
      // color: "lime",
      radius: "xs",
      position: "bottom-center"
    });

    writeText(fastClipRef.current.value)
      .then(() => {
        console.log("Text written successfully");

        if (fastClipRef.current.clear_time) {
          invoke('delay_clear_clipboard', { "delay": fastClipRef.current.clear_time })
            .catch((error) => console.error(error));
        }

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
      <Grid w="100%" justify="center" align="center">
        <Grid.Col span="auto">
          <Tooltip withArrow multiline label={
            <div style={{
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              width: '100%'
            }}>
              {fastClipRef.current.value}
            </div>
          } position="bottom" color="gray" w={width * 0.90}>

            <Button
              ref={buttonRef} // Reference for measuring width
              fullWidth
              autoContrast
              justify="space-between"
              leftSection={<IconTag size={14} />}
              variant="filled"
              color={fastClipRef.current.colour}
              radius="md"
              size="xs"
              onClick={handleCopy}

            >
              {fastClipRef.current.label}
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

      <Edit fastClipRef={fastClipRef} open={openEdit} close={closeEdit} opened={editOpened} />
      <Delete fastClipRef={fastClipRef} open={openDelete} close={closeDelete} opened={deleteOpened} />
    </>
  );
};

export default Clip;
