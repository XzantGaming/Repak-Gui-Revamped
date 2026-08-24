import React, { useState, useMemo } from 'react';
import { VscFolder, VscFolderOpened, VscFile } from 'react-icons/vsc';
import './FileTree.css';

type TreeNodeData = {
  id: string;
  name: string;
  fullPath?: string;
  children: TreeNodeData[];
  isFolder: boolean;
};

type FileTreeProps = {
  files?: string[];
};

const FileIcon = ({ name }: { name: string }) => {
  return <VscFile className="node-icon file-icon" />;
};

const FolderIcon = ({ isOpen }: { isOpen: boolean }) => {
  return isOpen ? 
    <VscFolderOpened className="node-icon folder-icon" /> : 
    <VscFolder className="node-icon folder-icon" />;
};

// Right-click copies the full path; modifiers narrow what lands on the clipboard.
const copyVariant = (fullPath: string, isFolder: boolean, nameOnly: boolean, stripExtension: boolean) => {
  let value = nameOnly ? (fullPath.split('/').pop() || fullPath) : fullPath;
  // Folder names can legitimately contain dots, so only files lose a suffix.
  if (stripExtension && !isFolder) {
    value = value.replace(/\.[^./]+$/, '');
  }
  return value;
};

const TreeNode = ({ node }: { node: TreeNodeData }) => {
  const [isOpen, setIsOpen] = useState(true); // Default to open for better visibility
  const [copiedLabel, setCopiedLabel] = useState<string | null>(null);
  const isFolder = node.isFolder;

  const handleToggle = (e: React.MouseEvent<HTMLDivElement>) => {
    e.stopPropagation();
    if (isFolder) {
      setIsOpen(!isOpen);
    }
  };

  const handleContextMenu = (e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();

    if (!node.fullPath) return;

    // Ctrl -> name only, Shift -> drop the extension. They combine.
    const nameOnly = e.ctrlKey || e.metaKey;
    const stripExtension = e.shiftKey;
    const value = copyVariant(node.fullPath, isFolder, nameOnly, stripExtension);

    const label = nameOnly ? 'Copied name' : 'Copied path';

    navigator.clipboard.writeText(value).then(() => {
      setCopiedLabel(stripExtension && !isFolder ? `${label} (no ext)` : label);
      setTimeout(() => setCopiedLabel(null), 1500);
    }).catch(err => console.error('Failed to copy:', err));
  };

  const copyHint = node.fullPath
    ? [
      'Right click to copy full path',
      'Ctrl + right click: name only',
      ...(isFolder ? [] : ['Shift + right click: without extension'])
    ].join('\n')
    : node.name;

  return (
    <div className="tree-node">
      <div 
        className={`node-content ${isFolder ? 'folder' : 'file'}`}
        onClick={handleToggle}
        onContextMenu={handleContextMenu}
        title={copyHint}
      >
        {isFolder ? <FolderIcon isOpen={isOpen} /> : <FileIcon name={node.name} />}
        <span className="node-label">{node.name}</span>
        {copiedLabel && <span className="copied-tooltip">{copiedLabel}</span>}
      </div>
      
      {isFolder && isOpen && (
        <div className="node-children">
          {node.children.map((child) => (
            <TreeNode key={child.id} node={child} />
          ))}
        </div>
      )}
    </div>
  );
};

const FileTree = ({ files }: FileTreeProps) => {
  const treeData = useMemo(() => {
    const root: TreeNodeData = { id: 'root', name: 'root', children: [], isFolder: true };
    let idCounter = 0;

    const fileList: string[] = Array.isArray(files) ? files : [];

    fileList.forEach((path) => {
      // Skip repak/iostore metadata files (chunknames, patched_files) - not real mod content
      const lowerPath = path.toLowerCase();
      if (lowerPath.includes('chunknames') || lowerPath.includes('patched_files')) {
        return;
      }

      // Normalize path separators
      let normalizedPath = path.replace(/\\/g, '/');

      // 1. Skip "Game" folder if it's at the start
      // Handle cases like "Game/Marvel/..." or just "Game" or "Game/"
      if (normalizedPath.startsWith('Game/')) {
        normalizedPath = normalizedPath.substring(5);
      } else if (normalizedPath === 'Game') {
        return; // Skip the folder itself if it's just "Game"
      }
      
      // Also handle cases where the path might be absolute or have leading slashes
      // e.g. "/Game/Marvel/..."
      if (normalizedPath.startsWith('/Game/')) {
        normalizedPath = normalizedPath.substring(6);
      }

      // Filter out empty parts to handle double slashes or trailing slashes
      const parts = normalizedPath.split('/').filter(p => p && p !== '.');
      
      // Double check if the first part is still "Game" (case insensitive?)
      if (parts.length > 0 && parts[0].toLowerCase() === 'game') {
         parts.shift();
      }

      if (parts.length === 0) return;

      let current = root;
      let currentPath = '';
      
      parts.forEach((part, index) => {
        const isFile = index === parts.length - 1;
        currentPath = currentPath ? `${currentPath}/${part}` : part;
        let child = current.children.find((c) => c.name === part);
        
        if (!child) {
          child = {
            id: `node-${idCounter++}`,
            name: part,
            fullPath: currentPath,
            children: [],
            isFolder: !isFile
          };
          current.children.push(child);
        }
        current = child;
      });
    });
    
    // Recursive sort
    const sortNode = (node: TreeNodeData) => {
      if (node.children) {
        node.children.sort((a, b) => {
          if (a.isFolder === b.isFolder) return a.name.localeCompare(b.name);
          return a.isFolder ? -1 : 1;
        });
        node.children.forEach(sortNode);
      }
    };
    sortNode(root);

    // 2. Merge single-child folders
    const mergeSingleChildFolders = (node: TreeNodeData) => {
      if (!node.children) return;

      // Process children first (bottom-up might be safer, but top-down works for merging downwards)
      // Actually, let's do top-down merging.
      
      // We iterate through children. If a child is a folder and has exactly one child which is also a folder, merge them.
      // However, we need to be careful about modifying the array while iterating.
      
      // Better approach: Recursively process children first, then try to merge current node with its single child if applicable?
      // No, the requirement is "merge the folders up to the first folder with assets/subfolders".
      // This means if A has only child B, and B is a folder, display as "A/B".
      
      // Let's do a post-order traversal (process children first) to ensure deep merges happen correctly?
      // Actually, pre-order (top-down) is fine for "A" -> "A/B" -> "A/B/C".
      
      for (let i = 0; i < node.children.length; i++) {
        let child = node.children[i];
        
        // While child is a folder and has exactly one child that is ALSO a folder
        while (
          child.isFolder && 
          child.children && 
          child.children.length === 1 && 
          child.children[0].isFolder
        ) {
          const grandChild = child.children[0];
          // Merge name
          child.name = `${child.name}/${grandChild.name}`;
          // Update fullPath to point to the deepest merged child
          child.fullPath = grandChild.fullPath;
          // Adopt grandchildren
          child.children = grandChild.children;
          // Update ID to ensure uniqueness if needed, or keep child's ID
        }
        
        // Recurse for the next level (which might now be several levels deep originally)
        mergeSingleChildFolders(child);
      }
    };

    mergeSingleChildFolders(root);
    
    return root.children;
  }, [files]);

  if (!files || files.length === 0) {
    return <div className="file-tree" style={{ padding: '1rem', color: 'var(--text-secondary)' }}>No files to display</div>;
  }

  return (
    <div className="file-tree">
      {treeData.map(node => (
        <TreeNode key={node.id} node={node} />
      ))}
    </div>
  );
};

export default FileTree;