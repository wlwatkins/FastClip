import { Box, Button } from "@mantine/core";
import { IconEdit, IconTrashX, IconTag } from "@tabler/icons-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import FastClip from "../Classes/FastClip";
import { useDisclosure } from "@mantine/hooks";
import Edit from "./EditClip";
import Delete from "./DeleteClip";
import { useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { notifications } from "@mantine/notifications";
import { ContextMenu } from "../Customs/ContextMenu";

interface ItemProps {
  fast_clip: FastClip;
}

const Clip: React.FC<ItemProps> = ({ fast_clip }) => {
  const fastClipRef = useRef<FastClip>(fast_clip);
  const [editOpened, { open: openEdit, close: closeEdit }] = useDisclosure(false);
  const [deleteOpened, { open: openDelete, close: closeDelete }] = useDisclosure(false);
  const [_copied, setCopied] = useState(false);
  const [opened, setOpened] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);


  const handlePutInClipBoard = () => {
    notifications.show({
      title: `Copied '${fastClipRef.current.label}' to clipboard`,
      message: undefined,
      withCloseButton: true,
      radius: "xs",
      position: "bottom-center"
    });

    writeText(fastClipRef.current.value)
      .then(() => {
        if (fastClipRef.current.clear_time) {
          invoke('delay_clear_clipboard', { "delay": fastClipRef.current.clear_time })
            .catch(console.error);
        }
      })
      .catch(console.error);
  };

  const handleCopy = () => {
    handlePutInClipBoard();
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };



  return (
    <Box 
    >


    <ContextMenu
      shadow="md"
      opened={opened}
      onChange={setOpened}
      closeOnClickOutside={true}
      closeOnEscape={true}
      position="bottom" 
      offset={20}
      withArrow
    >
      <ContextMenu.Target>
        <Button
          ref={buttonRef}
          fullWidth
          autoContrast
          justify="space-between"
          leftSection={<IconTag size={14} />}
          variant="filled"
          radius="md"
          size="xs"
          color={fastClipRef.current.colour}
          onClick={handleCopy}
        >
          <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis'}}>
            {fastClipRef.current.label}
          </span>
        </Button>


      </ContextMenu.Target>


      <ContextMenu.Dropdown>
        {/* <ContextMenu.Label>Application</ContextMenu.Label> */}
        <ContextMenu.Item leftSection={<IconEdit size={14} />} onClick={openEdit}>Edit clip</ContextMenu.Item>
        <ContextMenu.Item leftSection={<IconTrashX size={14} />} color="red" onClick={openDelete}>Delete</ContextMenu.Item>
      </ContextMenu.Dropdown>
    </ContextMenu>

    <Edit fastClipRef={fastClipRef} open={openEdit} close={closeEdit} opened={editOpened} />
    <Delete fastClipRef={fastClipRef} open={openDelete} close={closeDelete} opened={deleteOpened} />
  </Box>
  );
};

export default Clip;
