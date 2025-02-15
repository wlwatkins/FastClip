import { ActionIcon, Button, Grid, Tooltip } from "@mantine/core";
import { IconEdit, IconTrashX, IconEyeOff } from '@tabler/icons-react';
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import FastClip from "../Classes/FastClip";
import { useDisclosure } from "@mantine/hooks";
import Edit from "./EditClip";
import Delete from "./DeleteClip";
import * as motion from "motion/react-client"
import { useState } from "react";

interface ItemProps {
  fast_clip: FastClip;
}

const Clip: React.FC<ItemProps> = ({ fast_clip }) => {
  const [editOpened, { open: openEdit, close: closeEdit }] = useDisclosure(false);
  const [deleteOpened, { open: openDelete, close: closeDelete }] = useDisclosure(false);
  const [copied, setCopied] = useState(false);

  const handlePutInClipBoard = () => {
    writeText(fast_clip.value)
      .then(() => {
        console.log('Text written successfully');
      })
      .catch((error) => {
        console.error('Error writing text:', error);
      });
  };

  const handleCopy = () => {
    handlePutInClipBoard();
    setCopied(true);
    setTimeout(() => setCopied(false), 2000); // Reset after 2 seconds
  };


  return (
    <>
      <Grid w="90%">
        <Grid.Col span="auto">
          <motion.div>
            <Tooltip label={fast_clip.value} position="bottom" color="gray">
              <Button
                fullWidth
                variant="gradient"
                gradient={{ from: "blue", to: "cyan", deg: -45 }}
                radius="md"
                onClick={handleCopy}
              >
                <motion.span
                  initial={{ opacity: 1 }}
                  animate={{ opacity: copied ? 1 : 1 }}
                  transition={{ duration: 1.5 }}
                  key={copied ? "copied" : fast_clip.label}
                >
                  {copied ? "Copied!" : fast_clip.label}
                </motion.span>

                <motion.span
                  initial={{ opacity: 1 }}
                  animate={{ opacity: copied ? [1, 0] : 1 }}
                  transition={{ duration: 1.5 }}
                  key={!copied ? "copied" : fast_clip.label}
                  style={{ position: 'absolute', visibility: 'hidden' }}
                >
                  {fast_clip.label}
                </motion.span>
                
              </Button>
            </Tooltip>

          </motion.div>

        </Grid.Col>

        <Grid.Col span="content" >
          <ActionIcon.Group >

            <ActionIcon variant="light" size="lg" onClick={openEdit}>
              <IconEdit size={20} stroke={1.5} />
            </ActionIcon>

            <ActionIcon variant="light" color="red" size="lg" aria-label="Settings" onClick={openDelete}>
              <IconTrashX size={20} stroke={1.5} />
            </ActionIcon>

            <ActionIcon variant="light" color="gray" size="lg" aria-label="Likes">
              <IconEyeOff size={20} stroke={1.5} />
            </ActionIcon>

          </ActionIcon.Group>
        </Grid.Col>
      </Grid>

      <Edit fast_clip={fast_clip} open={openEdit} close={closeEdit} opened={editOpened} />
      <Delete fast_clip={fast_clip} open={openDelete} close={closeDelete} opened={deleteOpened} />
    </>
  )
}
export default Clip;