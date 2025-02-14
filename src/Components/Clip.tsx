import { ActionIcon, Button, Flex, Grid, Modal } from "@mantine/core";
import { IconEdit, IconTrashX, IconEyeOff } from '@tabler/icons-react';
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import FastClip from "../Classes/FastClip";
import { useDisclosure } from "@mantine/hooks";
import { invoke } from "@tauri-apps/api/core";
import Edit from "./EditClip";
import Delete from "./DeleteClip";

interface ItemProps {
  fast_clip: FastClip;
}

const Clip: React.FC<ItemProps> = ({ fast_clip }) => {
  const [editOpened, { open: openEdit, close: closeEdit }] = useDisclosure(false);
  const [deleteOpened, { open: openDelete, close: closeDelete }] = useDisclosure(false);

  const handlePutInClipBoard = () => {
    writeText(fast_clip.value)
      .then(() => {
        console.log('Text written successfully');
      })
      .catch((error) => {
        console.error('Error writing text:', error);
      });
  };






  return (

    <>
    


      <Grid w="90%">
        <Grid.Col span="auto">
          <Button 
          fullWidth 
          variant="gradient"
          gradient={{ from: 'blue', to: 'cyan', deg: -45 }}
          radius="md" 
          onClick={handlePutInClipBoard}>
            {fast_clip.label}
          </Button>
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
      <Delete fast_clip={fast_clip} open={openDelete} close={closeDelete} opened={deleteOpened}  />

    </>
  )
}
export default Clip;